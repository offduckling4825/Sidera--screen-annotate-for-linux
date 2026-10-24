# Xwayland 24.1.10 本地回补材料

这份材料用于当前 **Ubuntu 26.04 amd64 虚拟机**的兼容性修复验证。它把 [X.Org 官方修复](https://gitlab.freedesktop.org/xorg/xserver/-/commit/36f53145e415bbc959cdf2ed8bb0cb4f7c4c1f13) 回补到 Ubuntu `2:24.1.10-1` 源码，生成本地版本 `2:24.1.10-1+sidera1`。**这是本地构建，不是 Ubuntu 官方更新；不适用于 Ubuntu 24.04，也不适用于学校的 ARM64 麒麟电脑。**

原版在映射窗口产生背景重绘时可能于 `damage_report()` 解引用空指针，导致 Xwayland 崩溃并影响整个桌面。随附补丁只有 12 行差异：缺少关联窗口时直接返回，并在窗口初始化失败时注销 damage hook。原版崩溃地址已用匹配 Build ID 的官方调试符号确认。隔离显示服务器中已复现原版崩溃，同条件回补程序未崩溃；这不是对全部桌面功能或长期稳定性的保证。

## 材料与标记

- `0001-xwayland-damage-report-null.patch`：上游原始补丁，保留作者与提交信息。
- `SOURCE-SHA256SUMS`、`provenance.json`：Ubuntu 源码来源、SHA256、构建环境及本次产物校验值。
- `debian/rules`、`debian/changelog`：保留 Ubuntu 源包的构建配置，增加本地版本和服务器 vendor 标记。

修复程序的 X11 setup vendor 为 `The X.Org Foundation (Sidera damage-report fix)`，release number 仍为 **12401010**。Sidera 可据实际连接服务器的这些字段区分本回补版本和未修复版本，无需猜测本机软件包版本。标记仅表示这份明确包含补丁的构建，不应复制到未应用补丁的构建中。

仓库不保存二进制、DEB 或完整依赖清单，也不自动安装、替换服务器或更改桌面会话。本次包的实际依赖包括 `libc6 >= 2.43`、`libgcrypt20 >= 1.12.0`，由同发行版构建工具从产物生成。

## 在 Ubuntu 26.04 amd64 重建

先安装下文列出的构建依赖，再在独立构建目录操作。将 `materials` 改为本目录的绝对路径；以下只下载、校验和构建，不安装生成的 Xwayland 包。本目录固定使用 LF 换行，以保留补丁校验值。

```bash
materials=/absolute/path/to/repository/compat/xwayland-24.1.10
build_dir=$(mktemp -d /var/tmp/sidera-xwayland-build.XXXXXX)
cd "$build_dir"
pool=https://security.ubuntu.com/ubuntu/pool/main/x/xwayland
for name in xwayland_24.1.10-1.dsc xwayland_24.1.10.orig.tar.xz \
            xwayland_24.1.10.orig.tar.xz.asc xwayland_24.1.10-1.debian.tar.xz; do
    curl --fail --location --max-time 60 --output "$name" "$pool/$name" || exit 1
done
sha256sum --check "$materials/SOURCE-SHA256SUMS" || exit 1
printf '%s  %s\n' \
    6951494cbadaa33f4dcd02008c0bf83530dbc303c909dbfbe1718c317824b27a \
    "$materials/0001-xwayland-damage-report-null.patch" | sha256sum --check - || exit 1
dpkg-source -x xwayland_24.1.10-1.dsc source || exit 1
cp "$materials/0001-xwayland-damage-report-null.patch" source/debian/patches/
printf '%s\n' 0001-xwayland-damage-report-null.patch >> source/debian/patches/series
cp "$materials/debian/rules" source/debian/rules
cp "$materials/debian/changelog" source/debian/changelog
chmod 755 source/debian/rules
cd source
QUILT_PATCHES=debian/patches QUILT_PATCH_OPTS=--fuzz=0 quilt push -a || exit 1
dpkg-checkbuilddeps || exit 1
dpkg-buildpackage -b -uc -us -j4
```

需要同一 Ubuntu 发行版的编译工具、`dpkg-dev`、`curl`，以及 `.dsc` 中列出的 Build-Depends；使用 `dpkg-checkbuilddeps` 核对，不混用其他发行版的图形库。首次环境可安装这些构建依赖：

```bash
sudo apt-get install build-essential dpkg-dev curl debhelper-compat meson pkgconf quilt \
  libdecor-0-dev libdrm-dev libei-dev libepoxy-dev libgcrypt-dev libgbm-dev \
  libnvidia-egl-wayland-dev liboeffis-dev libpixman-1-dev libtirpc-dev libxcvt-dev \
  libxfont-dev libxkbfile-dev libxshmfence-dev libxv-dev libwayland-dev \
  mesa-common-dev x11proto-dev xfonts-utils xtrans-dev wayland-protocols
```

产物在构建目录下。先解包到临时目录，检查 `Xwayland -version` 和 vendor 标记，再在独立嵌套显示服务器中验证原触发场景。原构建的两个 XTS 测试因未提供外部 `XTEST_DIR` 被跳过，不能记为通过。依赖或编译器更新可能改变最终哈希，本目录保存的是可重建方法，不承诺逐字节复现。

## 来源验证与回滚

四个源文件来自 [Ubuntu 官方 pool](https://security.ubuntu.com/ubuntu/pool/main/x/xwayland/)，三个归档的 SHA256 也逐项匹配 `.dsc` 的 `Checksums-Sha256`。当时缺少 Debian 上传者公钥，未独立验证 `.dsc` 的 PGP 签名；此处不把 HTTPS 和哈希核对描述为签名认证。

原包为 `xwayland_24.1.10-1_amd64.deb`，其官方 URL 和 SHA256 见 `provenance.json`。宿主测试交付目录另外保留了原包，供受控回滚使用；**回滚也会恢复该已知缺陷**。本目录不提供自动安装或自动回滚步骤，以免在活动桌面中替换显示服务器。正式部署应优先采用发行版发布的修复更新。
