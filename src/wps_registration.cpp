// Sidera - WPS 加载项安全注册（自动注册和命令行安装共用）
// Copyright (C) 2026 Carl_Jin   GNU GPL v3
#include "wps_registration.h"

#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QLockFile>
#include <QSaveFile>
#include <QXmlStreamReader>
#include <QXmlStreamWriter>

namespace WpsRegistration {
QString registryPath() {
  return QDir::homePath() + "/.local/share/Kingsoft/wps/jsaddons/publish.xml";
}

static void writeEntry(QXmlStreamWriter& writer) {
  writer.writeEmptyElement("jspluginonline");
  writer.writeAttribute("name", "sidera-bridge");
  writer.writeAttribute("type", "wpp");
  writer.writeAttribute("url", "http://127.0.0.1:16666/");
  writer.writeAttribute("debug", "");
  writer.writeAttribute("enable", "enable");
  writer.writeAttribute("install", "null");
}

Result registerAddin(const QString& path, QString* error) {
  if (error) error->clear();
  const auto fail = [error](const QString& message) {
    if (error) *error = message;
    return Result::Failed;
  };
  if (!QDir().mkpath(QFileInfo(path).absolutePath()))
    return fail("无法创建 WPS 加载项目录");

  // 串行化本程序的自动注册和手动安装，避免相互覆盖。
  QLockFile lock(path + ".sidera.lock");
  if (!lock.tryLock(5000)) return fail("无法锁定 WPS 登记文件，请稍后重试");

  QByteArray content;
  QXmlStreamWriter writer(&content);
  if (QFileInfo::exists(path) || QFileInfo(path).isSymLink()) {
    QFile file(path);
    if (!file.open(QIODevice::ReadOnly))
      return fail("无法读取 WPS 登记文件：" + file.errorString());
    const QByteArray original = file.readAll();
    if (file.error() != QFileDevice::NoError)
      return fail("读取 WPS 登记文件失败：" + file.errorString());
    file.close();

    QXmlStreamReader reader(original);
    int depth = 0;
    bool found = false;
    while (!reader.atEnd()) {
      reader.readNext();
      if (reader.hasError()) break;
      if (reader.isStartElement()) {
        ++depth;
        if (depth == 1 && (reader.name() != "jsplugins" || !reader.namespaceUri().isEmpty()))
          return fail("WPS 登记文件根节点不是 jsplugins，已保留原文件");
        // 只识别真实的顶层加载项，不把注释、URL 或其它节点中的名称当作已安装。
        if (depth == 2 && reader.name() == "jspluginonline" && reader.namespaceUri().isEmpty()) {
          const QString name = reader.attributes().value("name").toString();
          if (name == "sidera-bridge" || name == "screen-annotate-bridge") found = true;
        }
      } else if (reader.isEndElement()) {
        if (depth == 1 && !found) {
          writer.writeCharacters("\n  ");
          writeEntry(writer);
          writer.writeCharacters("\n");
        }
        --depth;
      }
      writer.writeCurrentToken(reader);
    }
    // 即使前面已找到本加载项，也必须验证完整文档；空文件和损坏文件不得重建。
    if (reader.hasError())
      return fail(QString("WPS 登记文件 XML 解析失败（第 %1 行）：%2；已保留原文件")
                  .arg(reader.lineNumber()).arg(reader.errorString()));
    if (found) return Result::AlreadyRegistered;
  } else {
    writer.writeStartDocument("1.0", true);
    writer.writeStartElement("jsplugins");
    writer.writeCharacters("\n  ");
    writeEntry(writer);
    writer.writeCharacters("\n");
    writer.writeEndElement();
    writer.writeEndDocument();
  }
  if (writer.hasError()) return fail("生成 WPS 登记文件失败，已保留原文件");

  // 先写同目录临时文件，再原子替换；禁止退回直接截断原文件的写入方式。
  QSaveFile output(path);
  output.setDirectWriteFallback(false);
  // 禁用缓冲，write() 才会返回实际写入量，避免短写在 commit() 时被漏报。
  if (!output.open(QIODevice::WriteOnly | QIODevice::Unbuffered))
    return fail("无法保存 WPS 登记文件：" + output.errorString());
  if (output.write(content) != content.size()) {
    output.cancelWriting();
    return fail("写入 WPS 登记文件失败：" + output.errorString());
  }
  if (!output.commit()) return fail("提交 WPS 登记文件失败：" + output.errorString());
  return Result::Added;
}
}
