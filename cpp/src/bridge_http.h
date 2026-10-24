#pragma once
#include <QTcpSocket>
#include <QTimer>
#include <QFile>
#include <QFileInfo>
#include <QDir>
#include <QUrl>
#include <QUrlQuery>

namespace BridgeHttp {
struct Request { QByteArray method; QString path, message, session, sequence; };
inline void reply(QTcpSocket* socket, int status, const QByteArray& body, const QByteArray& type = "text/plain; charset=utf-8") {
  const QByteArray reason = status == 200 ? "OK" : status == 404 ? "Not Found" : status == 405 ? "Method Not Allowed" : "Request Rejected";
  const QByteArray origin = "http://127.0.0.1:" + QByteArray::number(socket->localPort());
  socket->setProperty("handled", true);
  socket->write("HTTP/1.1 " + QByteArray::number(status) + " " + reason + "\r\nAccess-Control-Allow-Origin: " + origin +
      "\r\nAccess-Control-Allow-Methods: GET, OPTIONS\r\nVary: Origin\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nContent-Type: " + type +
      "\r\nContent-Length: " + QByteArray::number(body.size()) + "\r\nConnection: close\r\n\r\n" + body);
  socket->disconnectFromHost();
}
inline void prepare(QTcpSocket* socket) {
  socket->setReadBufferSize(8193);
  QObject::connect(socket, &QTcpSocket::disconnected, socket, &QObject::deleteLater);
  QTimer::singleShot(500, socket, [socket]() { socket->abort(); socket->deleteLater(); });
}
inline bool read(QTcpSocket* socket, Request& request) {
  if (socket->property("handled").toBool()) return false;
  const QByteArray bytes = socket->peek(8193);
  if (bytes.size() > 8192) { reply(socket, 431, "Headers too large"); return false; }
  const int end = bytes.indexOf("\r\n\r\n");
  if (end < 0) return false;
  socket->read(end + 4);
  const auto lines = bytes.left(end).split('\n');
  const auto words = lines.first().trimmed().split(' ');
  if (words.size() != 3 || !words[1].startsWith('/') || words[1].startsWith("//") ||
      (words[2] != "HTTP/1.1" && words[2] != "HTTP/1.0")) { reply(socket, 400, "Bad request"); return false; }
  if (lines.first().size() > 4096) { reply(socket, 414, "URI too long"); return false; }
  if (words[0] != "GET" && words[0] != "OPTIONS") { reply(socket, 405, "GET required"); return false; }
  QByteArray host, origin;
  bool seenHost = false, seenOrigin = false;
  for (int i = 1; i < lines.size(); ++i) {
    const QByteArray line = lines[i].trimmed();
    const int colon = line.indexOf(':');
    if (colon < 1) { reply(socket, 400, "Bad header"); return false; }
    const QByteArray name = line.left(colon).toLower(), value = line.mid(colon + 1).trimmed();
    if (name == "host") { if (seenHost) { reply(socket, 400, "Duplicate host"); return false; } host = value; seenHost = true; }
    if (name == "origin") { if (seenOrigin) { reply(socket, 400, "Duplicate origin"); return false; } origin = value; seenOrigin = true; }
    if (name == "transfer-encoding" || (name == "content-length" && value != "0")) { reply(socket, 400, "Body not accepted"); return false; }
    if (name == "sec-fetch-site" && value == "cross-site") { reply(socket, 403, "Cross-site request"); return false; }
  }
  const QByteArray host1 = "127.0.0.1:" + QByteArray::number(socket->localPort());
  const QByteArray host2 = "localhost:" + QByteArray::number(socket->localPort());
  if ((host != host1 && host != host2) || (seenOrigin && origin != "http://" + host1 && origin != "http://" + host2)) {
    reply(socket, 403, "Origin or host rejected"); return false;
  }
  const QUrl url = QUrl::fromEncoded("http://" + host1 + words[1]);
  request.method = words[0]; request.path = url.path();
  request.message = QUrlQuery(url).queryItemValue("m", QUrl::FullyDecoded);
  request.session = QUrlQuery(url).queryItemValue("sid", QUrl::FullyDecoded);
  request.sequence = QUrlQuery(url).queryItemValue("seq", QUrl::FullyDecoded);
  if (request.message.size() > 2048) { reply(socket, 400, "Message too long"); return false; }
  for (const auto character : request.message) if (character.category() == QChar::Other_Control) {
    reply(socket, 400, "Invalid message"); return false;
  }
  return true;
}
inline bool staticPath(const QString& path) {
  return QStringList({"/", "/manifest.xml", "/ribbon.xml", "/index.html", "/main.js", "/js/bridge.js"}).contains(path);
}
inline void serve(QTcpSocket* socket, const QString& directory, const QString& path) {
  if (directory.isEmpty() || !staticPath(path)) { reply(socket, 404, "Not found"); return; }
  const QString root = QFileInfo(directory).canonicalFilePath();
  const QString relative = path == "/" ? "manifest.xml" : path.mid(1);
  const QFileInfo info(directory + "/" + relative);
  if (!info.isFile() || info.size() > 1024 * 1024 || !info.canonicalFilePath().startsWith(root + "/")) {
    reply(socket, 404, "Not found"); return;
  }
  QFile file(info.filePath());
  if (!file.open(QIODevice::ReadOnly)) { reply(socket, 404, "Not found"); return; }
  const QByteArray type = relative.endsWith(".xml") ? "application/xml" : relative.endsWith(".js") ? "text/javascript" : "text/html";
  reply(socket, 200, file.readAll(), type);
}
}
