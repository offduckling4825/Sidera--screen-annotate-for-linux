#include "wps_bridge.h"

#include <QCoreApplication>
#include <QElapsedTimer>
#include <QEvent>
#include <QEventLoop>
#include <QHostAddress>
#include <QPointer>
#include <QTcpServer>
#include <QTcpSocket>
#include <QThread>
#include <QTimer>
#include <cstdio>
#include <memory>
#include <stdexcept>

static void check(bool condition, const char* message) {
  if (!condition) throw std::runtime_error(message);
}

template <typename Predicate>
static void waitFor(Predicate ready, const char* message) {
  QElapsedTimer deadline;
  deadline.start();
  while (!ready() && deadline.elapsed() < 2000) {
    QCoreApplication::processEvents(QEventLoop::AllEvents, 10);
    QThread::msleep(1);
  }
  check(ready(), message);
}

static void drainDeletes() {
  QCoreApplication::sendPostedEvents(nullptr, QEvent::DeferredDelete);
}

static int activeTimers(const WpsBridge& bridge) {
  int active = 0;
  for (auto* timer : bridge.findChildren<QTimer*>())
    if (timer->isActive()) ++active;
  return active;
}

static void connectClient(WpsBridge& bridge, QTcpSocket& client) {
  client.connectToHost(QHostAddress::LocalHost, 16666);
  waitFor([&] {
    return client.state() == QAbstractSocket::ConnectedState &&
           !bridge.findChildren<QTcpSocket*>().isEmpty();
  }, "bridge did not accept client");
}

static QByteArray get(WpsBridge& bridge, const QByteArray& path) {
  QTcpSocket client;
  connectClient(bridge, client);
  client.write("GET " + path + " HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
  client.flush();
  waitFor([&] { return client.state() == QAbstractSocket::UnconnectedState; },
          "HTTP response did not finish");
  const QByteArray response = client.readAll();
  check(response.startsWith("HTTP/1.1 200 OK\r\n"), "unexpected HTTP response");
  const int body = response.indexOf("\r\n\r\n");
  check(body >= 0, "missing HTTP header terminator");
  drainDeletes();
  return response.mid(body + 4);
}

static void repeatedStartStop() {
  WpsBridge bridge;
  for (int i = 0; i < 100; ++i) {
    check(bridge.start(), "start failed during cycling");
    check(bridge.start(), "repeated start failed");
    check(activeTimers(bridge) == 1, "running bridge must have one active timer");
    bridge.stop();
    bridge.stop();
    check(activeTimers(bridge) == 0, "stopped bridge retained an active timer");
    drainDeletes();
    check(bridge.findChildren<QTimer*>().isEmpty(), "stopped timer was not released");
    check(bridge.findChildren<QTcpServer*>().isEmpty(), "server was not released");
    QTcpServer probe;
    check(probe.listen(QHostAddress::LocalHost, 16666), "stop did not release port");
  }
}

static void stopClosesAcceptedClient() {
  WpsBridge bridge;
  int callbacks = 0;
  bridge.onSlideshowBegin = [&](int) { ++callbacks; };
  check(bridge.start(), "start failed");
  QTcpSocket client;
  connectClient(bridge, client);
  QPointer<QTcpSocket> accepted = bridge.findChildren<QTcpSocket*>().first();
  // Queue a complete request without dispatching its readyRead event yet.
  client.write("GET /push?m=EVENT%20SlideShowBegin%20pos=3 HTTP/1.1\r\nHost: localhost\r\n\r\n");
  client.flush();
  bridge.stop();
  check(!accepted || accepted->state() == QAbstractSocket::UnconnectedState,
        "stop left an accepted client connected");
  check(bridge.start(), "immediate restart failed");
  waitFor([&] { return client.state() == QAbstractSocket::UnconnectedState; },
          "old client was not disconnected");
  drainDeletes();
  check(accepted.isNull(), "old client object survived deletion");
  check(callbacks == 0, "old client invoked a callback after stop/restart");
  check(!bridge.connected(), "old client changed restarted bridge state");
  bridge.stop();
  drainDeletes();
}

static void restartResetsSessionAndQueue() {
  WpsBridge bridge;
  int callbacks = 0;
  int lastPos = -1;
  bridge.onRealPos = [&](int pos) {
    lastPos = pos;
    ++callbacks;
  };
  const QByteArray page = "/push?m=EVENT%20SlideShowState%20pos=7%20click=0";
  check(bridge.start(), "start failed");
  check(get(bridge, page) == "OK", "page event rejected");
  check(callbacks == 1 && lastPos == 7 && bridge.connected(), "initial page not delivered");
  bridge.enqueue("NEXT");
  bridge.stop();
  check(!bridge.connected(), "stop retained online state");
  check(bridge.start(), "restart failed");
  check(get(bridge, page) == "OK", "restarted page event rejected");
  check(callbacks == 2 && lastPos == 7, "restart suppressed the same page from the new session");
  check(get(bridge, "/poll").isEmpty(), "restart retained an old command");
  bridge.stop();
  drainDeletes();
}

static void failedStartCanRecover() {
  QTcpServer blocker;
  check(blocker.listen(QHostAddress::LocalHost, 16666), "failed to reserve test port");
  WpsBridge bridge;
  check(!bridge.start(), "start unexpectedly succeeded on occupied port");
  bridge.stop();
  drainDeletes();
  check(bridge.findChildren<QTimer*>().isEmpty(), "failed start created a timer");
  check(bridge.findChildren<QTcpServer*>().isEmpty(), "failed server was not released");
  blocker.close();
  check(bridge.start(), "start did not recover after releasing port");
  check(activeTimers(bridge) == 1, "recovery must start one timer");
  bridge.stop();
  drainDeletes();
}

static void destructionReleasesResources() {
  std::unique_ptr<WpsBridge> bridge(new WpsBridge);
  check(bridge->start(), "start failed before destruction");
  QTcpSocket client;
  connectClient(*bridge, client);
  QPointer<QTimer> timer = bridge->findChildren<QTimer*>().first();
  QPointer<QTcpSocket> socket = bridge->findChildren<QTcpSocket*>().first();
  bridge.reset();
  check(timer.isNull() && socket.isNull(), "destruction retained child objects");
  waitFor([&] { return client.state() == QAbstractSocket::UnconnectedState; },
          "destruction did not disconnect client");
  QTcpServer probe;
  check(probe.listen(QHostAddress::LocalHost, 16666), "destruction retained port");
}

int main(int argc, char** argv) {
  QCoreApplication app(argc, argv);
  const struct { const char* name; void (*run)(); } cases[] = {
    {"100 start/stop cycles", repeatedStartStop},
    {"accepted client cleanup", stopClosesAcceptedClient},
    {"session and command reset", restartResetsSessionAndQueue},
    {"failed start recovery", failedStartCanRecover},
    {"destruction cleanup", destructionReleasesResources},
  };
  int failures = 0;
  for (const auto& test : cases) {
    try {
      test.run();
      std::printf("PASS %s\n", test.name);
    } catch (const std::exception& e) {
      ++failures;
      std::printf("FAIL %s: %s\n", test.name, e.what());
    }
    drainDeletes();
  }
  return failures ? 1 : 0;
}
