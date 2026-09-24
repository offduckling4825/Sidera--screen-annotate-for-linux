#pragma once

#include <QString>

namespace WpsRegistration {
enum class Result { Added, AlreadyRegistered, Failed };

QString registryPath();
Result registerAddin(const QString& path, QString* error = nullptr);
}
