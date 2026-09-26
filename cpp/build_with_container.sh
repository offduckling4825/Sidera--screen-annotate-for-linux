#!/bin/bash
# ============================================================
# 用仓库内的 Ubuntu 20.04 rootfs 容器编译（免 root：bwrap + qemu）
#   ./build_with_container.sh amd64     # 产出 annotate_amd64
#   ./build_with_container.sh aarch64   # 产出 annotate_aarch64（qemu 模拟，较慢）
#
# 说明：
#   - 两个容器是解压的 Ubuntu 20.04 根文件系统，含 gcc-9 + Qt5.12。
#   - 当前容器只装了 X11/Qt5 依赖；若要在容器内编译 Wayland 后端，
#     需先用 root 在容器内安装 libwayland-dev（见文件末尾 prepare 提示）。
#     未安装时 make 会自动退化为纯 X11 构建（不报错）。
# ============================================================
set -e

ARCH="${1:-amd64}"
ROOT="$(cd "$(dirname "$0")" && pwd)"       # .../repo/cpp
REPO="$(cd "$ROOT/.." && pwd)"              # 仓库根（含 protocols/、sysroot）

RUN=()
case "$ARCH" in
  amd64)
    SYSROOT="$REPO/amd64-sysroot"
    OUT="annotate_amd64"
    ;;
  aarch64)
    SYSROOT="$REPO/aarch64-sysroot"
    OUT="annotate_aarch64"
    QEMU="$(command -v qemu-aarch64-static || true)"
    if [ -z "$QEMU" ]; then echo "错误: 找不到 qemu-aarch64-static"; exit 1; fi
    RUN=(--ro-bind "$QEMU" /usr/bin/qemu-aarch64-static)
    ;;
  *)
    echo "用法: $0 {amd64|aarch64}"; exit 1;;
esac

if [ ! -d "$SYSROOT" ]; then echo "错误: 找不到 $SYSROOT"; exit 1; fi

BWRAP=(bwrap
  --ro-bind "$SYSROOT" /
  "${RUN[@]}"
  --dev /dev --proc /proc
  --bind "$REPO" /mnt
  --chdir /mnt/cpp
  --setenv HOME /root)

CMD="make clean >/dev/null 2>&1; make OUT=$OUT -j\"\$(nproc)\""

echo "==> 在 $ARCH 容器内编译 ($OUT) ..."
if [ "$ARCH" = aarch64 ]; then
  "${BWRAP[@]}" /usr/bin/qemu-aarch64-static /bin/bash -c "$CMD"
else
  "${BWRAP[@]}" /bin/bash -c "$CMD"
fi

echo "==> 完成: $ROOT/$OUT"
file "$ROOT/$OUT"

# ------------------------------------------------------------
# 【一次性准备】让容器内也能编译 Wayland 后端（需 root）。
#   容器缺 libwayland-dev / libwayland-bin，离线 deb 已放在 ~/wl-debs/。
#   直接进容器 dpkg -i 即可（不依赖容器网络/DNS）：
#
#   amd64:
#     sudo systemd-nspawn -D ~/cpp/amd64-sysroot \
#       --bind=$HOME/wl-debs:/debs \
#       dpkg -i /debs/libwayland-bin_1.18.0-1ubuntu0.1_amd64.deb \
#               /debs/libwayland-dev_1.18.0-1ubuntu0.1_amd64.deb
#
#   aarch64（宿主机 binfmt 已注册 qemu-aarch64，可直接跑）:
#     sudo systemd-nspawn -D ~/cpp/aarch64-sysroot \
#       --bind=$HOME/wl-debs:/debs \
#       dpkg -i /debs/libwayland-bin_1.18.0-1ubuntu0.1_arm64.deb \
#               /debs/libwayland-dev_1.18.0-1ubuntu0.1_arm64.deb
#
#   （若想改用 apt：进容器前加 --resolv-conf=off，进去后
#    rm -f /etc/resolv.conf && echo 'nameserver 223.5.5.5' > /etc/resolv.conf
#    再 apt-get update && apt-get install -y libwayland-dev）
#
#   装完用本脚本编译，日志里出现 -DSIDERA_HAVE_WAYLAND -lwayland-client 即成功。
# ------------------------------------------------------------
