#!/bin/bash
# ============================================================
# Rust 版 Sidera — amd64 (x86_64) 编译 + 打包（一步到位）
#
#   ./build_amd64.sh            编译 + 打包 DEB
#   ./build_amd64.sh --no-deb   只编译，不打包
#
# 依赖：
#   - bwrap
#   - rustup 工具链 stable-x86_64-unknown-linux-gnu
#   - 仓库根 amd64-sysroot/
#
# 产物：
#   target/container-amd64/release/sidera
#   ../sidera_3.0-Electro-testing_amd64.deb（打包时）
# ============================================================
set -e

HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE"

NO_DEB=0
if [ "$1" = "--no-deb" ]; then
  NO_DEB=1
fi

# ---------- 依赖检查 ----------
miss=0
command -v bwrap >/dev/null 2>&1 || { echo "缺少 bwrap"; miss=1; }
if [ ! -d "$HOME/.rustup/toolchains/stable-x86_64-unknown-linux-gnu" ]; then
  echo "缺少 Rust 工具链 stable-x86_64-unknown-linux-gnu"
  echo "  rustup toolchain install stable-x86_64-unknown-linux-gnu --profile minimal"
  miss=1
fi
if [ ! -d "$HERE/../amd64-sysroot" ]; then
  echo "缺少 $HERE/../amd64-sysroot"
  miss=1
fi
if [ "$miss" -ne 0 ]; then
  echo "依赖不满足，退出。"
  exit 1
fi

# ---------- 编译 ----------
echo "==> 编译 amd64（容器方式）..."
./build_container.sh amd64

# ---------- 打包 ----------
if [ "$NO_DEB" -eq 0 ]; then
  echo "==> 打包 amd64 DEB..."
  ./build_deb.sh amd64
else
  echo "==> 已跳过 DEB 打包（--no-deb）"
fi

echo "==> 完成。"
