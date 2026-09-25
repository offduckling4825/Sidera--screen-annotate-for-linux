#pragma once
// ============================================================
// Sidera - WPS 联动桥（平台无关：本地 HTTP 16666 + 加载项注册 + 心跳）
// 与 X11/Wayland 无关，纯 TCP + 事件回调。Wayland 后端使用。
// Copyright (C) 2026 Carl_Jin   GNU GPL v3
// ============================================================
#include <QObject>
#include <QString>
#include <QQueue>
#include <functional>

class QTcpServer;
class QTcpSocket;
class QTimer;

class WpsBridge : public QObject {
public:
  explicit WpsBridge(QObject* parent = nullptr);
  ~WpsBridge() override;

  bool start();                      // 监听 127.0.0.1:16666，并自动登记加载项
  void stop();
  void enqueue(const QString& cmd);  // NEXT / PREV
  bool connected() const { return connected_; }
  QString logFile() const;

  // 回调（由使用者设置）
  std::function<void(int)> onRealPos;        // 真实页号变化（去重后）
  std::function<void(int)> onSlideshowBegin; // 放映开始

private:
  void onNewConnection();
  void handleHttp(QTcpSocket* s);
  void httpReply(QTcpSocket* s, const QString& body);
  void serveAddinFile(QTcpSocket* s, const QString& path);
  void handleLine(const QString& raw);
  void touch();
  void ensureAddinRegistered();
  QString addinDir() const;
  void log(const QString& msg);

  QTcpServer* server_ = nullptr;
  QTimer*     ping_   = nullptr;
  QQueue<QString> queue_;
  bool    connected_ = false;
  qint64  lastSeen_  = 0;
  int     realPos_   = -1;
};
