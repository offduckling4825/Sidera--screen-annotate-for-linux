Name:           sidera
Version:        3.0
Release:        1
Summary:        Linux 屏幕批注与教学演示工具（Rust，X11/Wayland，WPS 联动）

License:        GPL-3.0-only
URL:            https://github.com/offduckling4825/Sidera--screen-annotate-for-linux
Source0:        sidera-files.tar.gz

# 只重新打包已编译好的二进制，不做任何编译/调试符号处理
%global debug_package %{nil}
%global __os_install_post %{nil}

Requires:       glibc >= 2.30

%description
Sidera 星罗 —— 面向 Linux 教室与演示的屏幕批注工具，纯 Rust 实现，
支持 X11 与 Wayland（wlr-layer-shell），点击穿透、手背/多指橡皮、白板，
并与 WPS 演示联动（本地 HTTP 桥 127.0.0.1:16666 + JS 加载项）。

%prep

%install
mkdir -p %{buildroot}
tar xf %{SOURCE0} -C %{buildroot}

%files
/usr/bin/sidera
/usr/bin/sidera-wps-addin-install
/usr/share/sidera
/usr/share/applications/sidera.desktop
/usr/share/icons/hicolor/scalable/apps/sidera.svg
/usr/share/icons/hicolor/256x256/apps/sidera.png
/usr/share/pixmaps/sidera.svg
/usr/lib/udev/rules.d/70-sidera-uinput.rules
/usr/lib/udev/rules.d/70-sidera-input.rules
/etc/modules-load.d/sidera-uinput.conf
/usr/share/licenses/sidera/LICENSE
/usr/share/doc/sidera/README.classroom.md

%post
modprobe uinput 2>/dev/null || :
udevadm control --reload-rules 2>/dev/null || :
udevadm trigger --subsystem-match=input 2>/dev/null || :

%postun
udevadm control --reload-rules 2>/dev/null || :

%changelog
* Fri Oct 02 2026 Carl_Jin <user@localhost> - 3.0-1
- Rust 版（Electro）
