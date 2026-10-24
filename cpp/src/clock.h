#pragma once
#include <chrono>
#include <QtGlobal>
inline qint64 monotonicMs() {
  return std::chrono::duration_cast<std::chrono::milliseconds>(
      std::chrono::steady_clock::now().time_since_epoch()).count();
}
