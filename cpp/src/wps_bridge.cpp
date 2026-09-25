// ============================================================
// Sidera - WPS 联动桥实现（提取自 wps.cpp 的 HTTP 部分，平台无关）
// Copyright (C) 2026 Carl_Jin   GNU GPL v3
// ============================================================
#include "wps_bridge.h"

#include <QTcpServer>
#include <QTcpSocket>
#include <QHostAddress>
#include <QTimer>
#include <QDateTime>
#include <QFile>
#include <QFileInfo>
#include <QDir>
#include <QTextStream>
#include <QCoreApplication>
#include <QUrl>
#include <QUrlQuery>
#include <QDebug>
#include <QRegularExpression>

WpsBridge::WpsBridge(QObject* parent) : QObject(parent) {}
WpsBridge::~WpsBridge() { stop(); }

QString WpsBridge::logFile() const {
  QString dir = QDir::homePath();
  if (!QFileInfo(dir).isWritable()) dir = "/tmp";
  return dir + "/wps-api-debug.log";
}

void WpsBridge::log(const QString& msg) {
  qDebug().noquote() << "[WPSAPI]" << msg;
  QString path = logFile();
  if (QFileInfo(path).size() > 1024 * 1024) QFile::remove(path);
  QFile f(path);
  if (f.open(QIODevice::Append | QIODevice::Text)) {
    QTextStream ts(&f);
    ts << QDateTime::currentDateTime().toString("yyyy-MM-dd HH:mm:ss.zzz ") << msg << "\n";
    f.close();
  }
}

QString WpsBridge::addinDir() const {
  QString env = QString::fromLocal8Bit(qgetenv("WPS_ADDIN_DIR"));
  if (!env.isEmpty() && QFile::exists(env + "/manifest.xml")) return env;
  QString app = QCoreApplication::applicationDirPath() + "/wps-addin";
  if (QFile::exists(app + "/manifest.xml")) return app;
  QString sys = "/usr/share/sidera/wps-addin";
  if (QFile::exists(sys + "/manifest.xml")) return sys;
  return QString();
}

void WpsBridge::ensureAddinRegistered() {
  QString path = QDir::homePath() + "/.local/share/Kingsoft/wps/jsaddons/publish.xml";
  QString entry = "  <jspluginonline name=\"sidera-bridge\" type=\"wpp\" "
                  "url=\"http://127.0.0.1:16666/\" debug=\"\" enable=\"enable\" install=\"null\"/>\n";
  QDir().mkpath(QFileInfo(path).absolutePath());
  QFile f(path);
  QString content;
  if (f.open(QIODevice::ReadOnly | QIODevice::Text)) { content = QString::fromUtf8(f.readAll()); f.close(); }
  if (content.contains("sidera-bridge") || content.contains("screen-annotate-bridge")) return;
  if (!content.isEmpty() && content.contains("<jsplugins>") && content.contains("</jsplugins>"))
    content.replace("</jsplugins>", entry + "</jsplugins>");
  else
    content = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<jsplugins>\n" + entry + "</jsplugins>\n";
  if (f.open(QIODevice::WriteOnly | QIODevice::Text)) {
    QTextStream ts(&f);
    ts << content;
    f.close();
    log("已自动登记加载项 → " + path);
  }
}

bool WpsBridge::start() {
  if (server_) return true;
  server_ = new QTcpServer(this);
  connect(server_, &QTcpServer::newConnection, this, [this]() { onNewConnection(); });
  if (!server_->listen(QHostAddress::LocalHost, 16666)) {
    log("HTTP 16666 绑定失败: " + server_->errorString());
    server_->deleteLater();
    server_ = nullptr;
    return false;
  }
  log("WPS 桥已启动，监听 127.0.0.1:16666");
  ensureAddinRegistered();

  ping_ = new QTimer(this);
  connect(ping_, &QTimer::timeout, this, [this]() {
    if (connected_ && QDateTime::currentMSecsSinceEpoch() - lastSeen_ > 3000) {
      connected_ = false;
      realPos_ = -1;
      log("客户端离线（3s 无请求）");
    }
  });
  ping_->start(1000);
  return true;
}

void WpsBridge::stop() {
  if (server_) { server_->close(); server_->deleteLater(); server_ = nullptr; }
  ping_ = nullptr;
  connected_ = false;
  queue_.clear();
}

void WpsBridge::enqueue(const QString& cmd) { queue_.enqueue(cmd); }

void WpsBridge::onNewConnection() {
  while (server_ && server_->hasPendingConnections()) {
    QTcpSocket* s = server_->nextPendingConnection();
    connect(s, &QTcpSocket::readyRead, this, [this, s]() { handleHttp(s); });
    connect(s, QOverload<QAbstractSocket::SocketError>::of(&QAbstractSocket::error),
            this, [s](QAbstractSocket::SocketError) { s->deleteLater(); });
  }
}

void WpsBridge::touch() {
  lastSeen_ = QDateTime::currentMSecsSinceEpoch();
  if (!connected_) { connected_ = true; log("客户端接入"); }
}

void WpsBridge::httpReply(QTcpSocket* s, const QString& body) {
  QByteArray b = body.toUtf8();
  QString resp =
      "HTTP/1.1 200 OK\r\n"
      "Access-Control-Allow-Origin: *\r\n"
      "Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n"
      "Access-Control-Allow-Headers: *\r\n"
      "Content-Type: text/plain; charset=utf-8\r\n"
      "Content-Length: " + QString::number(b.size()) + "\r\n"
      "Connection: close\r\n\r\n";
  s->write(resp.toUtf8() + b);
  s->flush();
  s->disconnectFromHost();
  s->deleteLater();
}

void WpsBridge::serveAddinFile(QTcpSocket* s, const QString& path) {
  QString dir = addinDir();
  if (dir.isEmpty()) { httpReply(s, "OK"); return; }
  QString rel = path.mid(1);
  if (rel.isEmpty()) rel = "manifest.xml";
  if (rel.contains("..") || rel.contains("//")) { httpReply(s, "OK"); return; }
  QString fp = dir + "/" + rel;
  if (!QFile::exists(fp) || QFileInfo(fp).isDir()) { httpReply(s, "OK"); return; }
  QFile f(fp);
  if (!f.open(QIODevice::ReadOnly)) { httpReply(s, "OK"); return; }
  QByteArray body = f.readAll();
  f.close();
  QString ct = "text/plain";
  if (rel.endsWith(".xml")) ct = "application/xml";
  else if (rel.endsWith(".js")) ct = "text/javascript";
  else if (rel.endsWith(".html") || rel.endsWith(".htm")) ct = "text/html";
  else if (rel.endsWith(".svg")) ct = "image/svg+xml";
  else if (rel.endsWith(".json")) ct = "application/json";
  QString resp =
      "HTTP/1.1 200 OK\r\n"
      "Access-Control-Allow-Origin: *\r\n"
      "Content-Type: " + ct + "; charset=utf-8\r\n"
      "Content-Length: " + QString::number(body.size()) + "\r\n"
      "Connection: close\r\n\r\n";
  s->write(resp.toUtf8() + body);
  s->flush();
  s->disconnectFromHost();
  s->deleteLater();
}

void WpsBridge::handleHttp(QTcpSocket* s) {
  if (!s->canReadLine()) return;
  QList<QByteArray> parts = s->readLine().split(' ');
  if (parts.size() < 2) { s->deleteLater(); return; }
  QByteArray method = parts[0];
  QUrl url = QUrl::fromEncoded("http://x" + parts[1]);
  QString path = url.path();
  QString m = QUrlQuery(url).queryItemValue("m");
  touch();
  if (method == "OPTIONS") { httpReply(s, ""); return; }
  if (path == "/hello") { log("加载项问候: " + m); httpReply(s, "OK sidera"); return; }
  if (path == "/push" && !m.isEmpty()) { handleLine(m); httpReply(s, "OK"); return; }
  if (path == "/poll") {
    QString cmd = queue_.isEmpty() ? QString() : queue_.dequeue();
    httpReply(s, cmd);
    return;
  }
  serveAddinFile(s, path);
}

void WpsBridge::handleLine(const QString& raw) {
  QString line = raw.trimmed();
  if (line.isEmpty()) return;
  log("收 << " + line);
  if (!line.startsWith("EVENT ")) return;
  int pos = -1, click = -1;
  QRegularExpression rxPos("pos=(\\d+)"), rxClick("click=(\\d+)");
  auto mPos = rxPos.match(line);   if (mPos.hasMatch()) pos = mPos.captured(1).toInt();
  auto mClick = rxClick.match(line); if (mClick.hasMatch()) click = mClick.captured(1).toInt();
  QString name = line.section(' ', 1, 1);
  log(QString("事件 %1 pos=%2 click=%3").arg(name).arg(pos).arg(click));
  if (name == "SlideShowBegin") {
    realPos_ = pos > 0 ? pos : 1;
    if (onSlideshowBegin) onSlideshowBegin(realPos_);
    return;
  }
  if (pos > 0 && pos != realPos_) {
    realPos_ = pos;
    if (onRealPos) onRealPos(pos);
  }
}
