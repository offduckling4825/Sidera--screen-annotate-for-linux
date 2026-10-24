#pragma once
#include <QPixmap>
#include <QImage>
#include <QMap>
#include <QTemporaryDir>
#include <QSaveFile>
#include <QDataStream>
#include <QDebug>
#include <memory>

// Preserve all visited pages. Compress premultiplied pixels losslessly, then
// spill beyond 32 MiB to private temporary files. Failed writes retain RAM data.
class PageCache {
  QMap<int, QByteArray> memory_;
  QMap<int, QPixmap> fallback_;
  QMap<int, QString> files_;
  std::unique_ptr<QTemporaryDir> directory_;
  qint64 budget_;
public:
  explicit PageCache(qint64 budget = 32 * 1024 * 1024) : budget_(budget) {}
  bool contains(int page) const { return memory_.contains(page) || files_.contains(page) || fallback_.contains(page); }
  int size() const { return memory_.size() + files_.size() + fallback_.size(); }
  void remove(int page) {
    memory_.remove(page);
    fallback_.remove(page);
    if (files_.contains(page)) QFile::remove(files_.take(page));
  }
  void clear() {
    memory_.clear(); fallback_.clear(); files_.clear(); directory_.reset();
  }
  void insert(int page, const QPixmap& pixmap) {
    const QImage image = pixmap.toImage().convertToFormat(QImage::Format_ARGB32_Premultiplied);
    QByteArray raw;
    QDataStream stream(&raw, QIODevice::WriteOnly);
    stream << qint32(image.width()) << qint32(image.height());
    for (int y = 0; y < image.height(); ++y)
      stream.writeRawData(reinterpret_cast<const char*>(image.constScanLine(y)), image.width() * 4);
    const QByteArray compressed = qCompress(raw, 1);
    if (compressed.isEmpty() || stream.status() != QDataStream::Ok) {
      remove(page); fallback_.insert(page, pixmap);
      qWarning() << "Cannot compress annotation page; retaining pixels in memory";
      return;
    }
    remove(page); memory_.insert(page, compressed);
    qint64 bytes = 0; for (const auto& data : memory_) bytes += data.size();
    if (bytes <= budget_) return;
    if (!directory_) directory_.reset(new QTemporaryDir);
    if (!directory_->isValid()) { qWarning() << "Page cache stays in memory: temporary storage unavailable"; return; }
    for (auto it = memory_.begin(); it != memory_.end() && bytes > budget_;) {
      const QString path = directory_->filePath(QString::number(it.key()) + ".page");
      QSaveFile file(path);
      if (!file.open(QIODevice::WriteOnly) || file.write(it.value()) != it.value().size() || !file.commit()) {
        qWarning() << "Page cache stays in memory:" << file.errorString(); break;
      }
      bytes -= it.value().size(); files_.insert(it.key(), path); it = memory_.erase(it);
    }
  }
  QPixmap value(int page) const {
    if (fallback_.contains(page)) return fallback_.value(page);
    QByteArray compressed = memory_.value(page);
    if (files_.contains(page)) {
      QFile file(files_.value(page));
      if (file.open(QIODevice::ReadOnly)) compressed = file.readAll();
      else qWarning() << "Cannot read cached annotation page:" << file.errorString();
    }
    if (compressed.isEmpty()) return {};
    QDataStream header(compressed); quint32 size = 0; header >> size;
    if (size < 8 || size > 256u * 1024 * 1024 + 8) return {};
    QByteArray raw = qUncompress(compressed);
    QDataStream stream(raw); qint32 width = 0, height = 0; stream >> width >> height;
    if (width < 1 || height < 1 || qint64(width) * height * 4 != raw.size() - 8) return {};
    QImage image(reinterpret_cast<const uchar*>(raw.constData() + 8), width, height, width * 4, QImage::Format_ARGB32_Premultiplied);
    return QPixmap::fromImage(image.copy());
  }
};
