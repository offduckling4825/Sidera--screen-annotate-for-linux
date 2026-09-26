#!/bin/bash
# ============================================================
# Rust 版 Sidera DEB 打包脚本
#   用法: ./build_deb.sh [amd64|aarch64]
#   产物: <repo>/sidera_3.0-Electro-testing_<amd64|arm64>.deb
#
# 依赖：dpkg-deb（Debian/Ubuntu 自带）
# 前置：先用 ./build_container.sh <arch> 编译出二进制
#       （或普通 cargo build --release，用 target/release 兜底）
#
# 与 C++ 版打包脚本的区别：
#   - 二进制来自 Rust 构建（target/container-<arch>/release/sidera）
#   - 运行时不依赖 Qt / libX11，只声明 libc6 + libgcc-s1
# ============================================================
set -e

ARG="${1:-amd64}"
HERE="$(cd "$(dirname "$0")" && pwd)"      # .../cpp/rust
REPO="$(cd "$HERE/.." && pwd)"             # .../cpp

case "$ARG" in
  amd64)        SRC_ARCH=amd64;  DEB_ARCH=amd64;;
  aarch64|arm64) SRC_ARCH=aarch64; DEB_ARCH=arm64;;
  *) echo "用法: $0 [amd64|aarch64]"; exit 1;;
esac

PKG_NAME="sidera"
# 注意：dpkg 要求 Version 以数字开头，故采用 "3.0-Electro-testing"；
# 如需改版本只改这里即可。
VERSION="3.0-Electro-testing"

# 优先容器构建产物，其次普通 release
BIN="$HERE/target/container-$SRC_ARCH/release/sidera"
[ -f "$BIN" ] || BIN="$HERE/target/release/sidera"
if [ ! -f "$BIN" ]; then
  echo "错误: 找不到 $SRC_ARCH 二进制，请先运行: ./build_container.sh $ARG"
  exit 1
fi

PKG_DIR="$REPO/${PKG_NAME}_${VERSION}_${DEB_ARCH}"
echo "==> 打包 $BIN -> ${PKG_DIR}.deb"

rm -rf "$PKG_DIR"
mkdir -p "$PKG_DIR/DEBIAN" \
         "$PKG_DIR/usr/bin" \
         "$PKG_DIR/usr/share/applications" \
         "$PKG_DIR/usr/share/sidera/wps-addin" \
         "$PKG_DIR/usr/share/doc/sidera" \
         "$PKG_DIR/usr/share/icons/hicolor/scalable/apps" \
         "$PKG_DIR/usr/share/icons/hicolor/256x256/apps" \
         "$PKG_DIR/usr/share/pixmaps"

# ---------- 二进制 ----------
install -m 755 "$BIN" "$PKG_DIR/usr/bin/sidera"

# ---------- WPS 加载项（放到固定路径，Rust 版会自动从 /usr/share/sidera/wps-addin 读取）----------
cp -r "$REPO/wps-addin/." "$PKG_DIR/usr/share/sidera/wps-addin/"
chmod 755 "$PKG_DIR/usr/share/sidera/wps-addin/install.sh"
install -m 755 "$REPO/wps-addin/install.sh" "$PKG_DIR/usr/bin/sidera-wps-addin-install"

# ---------- 文档 ----------
cp "$REPO/README.classroom.md" "$PKG_DIR/usr/share/doc/sidera/README.classroom.md"
cp "$REPO/LICENSE" "$PKG_DIR/usr/share/doc/sidera/copyright"
cp "$REPO/LICENSE" "$PKG_DIR/usr/share/doc/sidera/LICENSE"

# ---------- 图标 ----------
cp "$REPO/sidera.svg" "$PKG_DIR/usr/share/icons/hicolor/scalable/apps/sidera.svg"
cp "$REPO/sidera.svg" "$PKG_DIR/usr/share/pixmaps/sidera.svg"
cp "$REPO/sidera.png" "$PKG_DIR/usr/share/icons/hicolor/256x256/apps/sidera.png"

# ---------- uinput 权限（KWin 等无虚拟键盘协议时，用内核虚拟键盘发翻页键）----------
mkdir -p "$PKG_DIR/lib/udev/rules.d" "$PKG_DIR/etc/modules-load.d"
cat > "$PKG_DIR/lib/udev/rules.d/70-sidera-uinput.rules" << 'EOF'
# Sidera: 允许当前登录用户访问 /dev/uinput（用于注入翻页/退出按键）
KERNEL=="uinput", TAG+="uaccess", OPTIONS+="static_node=uinput"
EOF
echo "uinput" > "$PKG_DIR/etc/modules-load.d/sidera-uinput.conf"

cat > "$PKG_DIR/DEBIAN/postinst" << 'EOF'
#!/bin/sh
set -e
modprobe uinput 2>/dev/null || true
udevadm control --reload-rules 2>/dev/null || true
udevadm trigger --name-match=uinput 2>/dev/null || true
exit 0
EOF
chmod 755 "$PKG_DIR/DEBIAN/postinst"

# ---------- DEBIAN/control ----------
cat > "$PKG_DIR/DEBIAN/control" << EOF
Package: ${PKG_NAME}
Version: ${VERSION}
Section: graphics
Priority: optional
Architecture: ${DEB_ARCH}
Maintainer: User <user@localhost>
Depends: libc6 (>= 2.30), libgcc-s1
Description: Sidera (Rust) 屏幕批注工具
 纯 Rust 实现，不依赖 Qt / libX11，仅依赖 libc 与 libgcc。
 全屏透明画布批注，支持画笔/橡皮/白板，触控屏友好。
 内置 WPS 演示联动（本地 HTTP 127.0.0.1:16666）。
 安装后执行一次 sidera-wps-addin-install 注册加载项。
EOF

# ---------- desktop 入口 ----------
cat > "$PKG_DIR/usr/share/applications/sidera.desktop" << EOF
[Desktop Entry]
Type=Application
Name=Sidera
Comment=Sidera软件
Exec=sidera
Icon=/usr/share/pixmaps/sidera.svg
Categories=Graphics;Utility;
Terminal=false
EOF

# ---------- 打包 ----------
dpkg-deb --build --root-owner-group "$PKG_DIR"

echo ""
echo "========== 打包完成 =========="
ls -lh "${PKG_DIR}.deb"
echo ""
echo "安装: sudo dpkg -i ${PKG_DIR}.deb"
echo "卸载: sudo dpkg -r ${PKG_NAME}"
