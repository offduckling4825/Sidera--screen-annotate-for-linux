#!/bin/bash
# 安全合并当前用户的 WPS 加载项登记，保留其它加载项
# 使用加载项时需开启 app 的“WPS 接口调试模式”（内容由 16666 服务提供）
set -euo pipefail
SCRIPT_DIR="$(dirname "$(readlink -f "$0")")"
# 源码目录优先用本机架构的构建产物；deb 安装后使用 PATH 中的 sidera。
case "$(uname -m)" in
  x86_64) BINARY="$SCRIPT_DIR/../annotate_amd64" ;;
  aarch64|arm64) BINARY="$SCRIPT_DIR/../annotate_aarch64" ;;
  *) BINARY="" ;;
esac
if [ -z "$BINARY" ] || [ ! -x "$BINARY" ]; then
  BINARY="$(command -v sidera || true)"
fi
if [ -z "$BINARY" ]; then
  echo "错误：请先编译或安装 Sidera，再运行加载项安装脚本。" >&2
  exit 1
fi
"$BINARY" --register-wps-addin
echo "提示：请先启动“Sidera”并开启 设置→WPS接口调试，再重新打开 WPS 演示。"
echo "      放映时加载项会把真实页号回传，批注缓存随真实换页切换。"
