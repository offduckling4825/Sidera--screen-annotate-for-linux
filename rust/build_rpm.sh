#!/bin/bash
# ============================================================
# Rust 版 Sidera — 生成 RPM（主机 rpmbuild，二进制来自容器编译）
#
#   ./build_rpm.sh [amd64|aarch64]     # 默认 amd64
#
# 依赖：rpmbuild（主机）、tar
# 产物：rust/dist/sidera-<ver>-<rel>.<arch>.rpm
#
# 说明：不重新编译；直接打包 target/container-<arch>/release/sidera。
#       用 rpmbuild --target 支持 aarch64 交叉打包（无编译，仅重新打包）。
# ============================================================
set -e

HERE="$(cd "$(dirname "$0")" && pwd)"
source "$HERE/pkg_common.sh"

ARCH="${1:-amd64}"
pkg_parse_arch "$ARCH" || exit 1

BIN="$(pkg_find_bin)"
[ -f "$BIN" ] || { echo "错误: 找不到二进制 $BIN，请先 ./build_container.sh $SRC_ARCH"; exit 1; }
command -v rpmbuild >/dev/null || { echo "错误: 缺少 rpmbuild"; exit 1; }

DIST="$HERE/dist"
mkdir -p "$DIST"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
STAGE="$WORK/root"
stage_files "$BIN" "$STAGE"

echo "==> 打包 RPM ($PKG_ARCH)..."
tar czf "$WORK/sidera-files.tar.gz" -C "$STAGE" .

TOP="$WORK/rpmbuild"
mkdir -p "$TOP/BUILD" "$TOP/RPMS" "$TOP/SOURCES" "$TOP/SPECS" "$TOP/SRPMS"
cp "$WORK/sidera-files.tar.gz" "$TOP/SOURCES/"
cp "$HERE/rpm/sidera.spec" "$TOP/SPECS/"

rpmbuild -bb --target "$RPM_ARCH" --define "_topdir $TOP" "$TOP/SPECS/sidera.spec" \
  >/dev/null
cp "$TOP"/RPMS/*/*.rpm "$DIST/"

echo "==> 完成，产物："
ls -lh "$DIST"/sidera-*.rpm
