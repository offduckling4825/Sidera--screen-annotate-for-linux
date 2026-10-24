# 可开关的 Wayland 测试桌面

本目录在现有 Ubuntu Wayland 桌面中打开一个独立 Sway 窗口。无需注销、重启或切换默认桌面；适合对照 Sidera 的原生 Wayland 和 XWayland 两条路径。当前已在 VMware Ubuntu 26.04.1、Sway 1.11 / wlroots 0.19.2 上验证。

## 使用

1. 先保存并退出原桌面的 Sidera，避免同时显示两套工具栏。
2. 在应用菜单搜索 **Wayland 测试桌面**。默认启动原生 Wayland；该按钮启动的 Sidera 明确设置 `SIDERA_BACKEND=wayland`，并移除 `DISPLAY`，不能静默回退到 X11。
3. 选画笔画线，点箭头，再点击中间的穿透测试区域。正常情况下“按下”和“松开”计数同时增加。
4. 用 **XWayland 对照** 切换同一程序的 X11 后端；用 100% / 125% / 150% / 200% 调整缩放。切换后端会退出测试中的 Sidera 并重新启动；缩放会重建当前画布。
5. 点击 **结束测试，返回 Ubuntu**。也可在原桌面应用菜单选择 **关闭 Wayland 测试桌面**；测试窗口内的 `Ctrl+Shift+Escape` 同样退出。

200% 下可向下滚动面板。XWayland 对照仍运行在 Wayland 桌面中，不能代替原生 X11 会话验收。GNOME 本身仍不提供 layer-shell；安装测试桌面不会使 GNOME 获得该协议。

## 安装与日志

在目标 Linux 桌面准备依赖后，从本目录安装：

```sh
sudo apt install --no-install-recommends sway swaybg grim bubblewrap dbus-daemon python3-gi gir1.2-gtk-4.0
sudo bash install.sh
```

启动器为 `/usr/local/bin/sidera-wayland-lab`，面板与配置在 `/usr/local/share/sidera-wayland-lab/`。命令行 `sidera-wayland-lab --stop` 只访问这个测试环境自己的 Sway 通信文件。

每次启动创建新的测试家目录、运行目录、进程/网络空间和 D-Bus。测试中的单实例锁、设置、WPS 登记和本地服务与正常桌面隔离；不会导入或修改正常桌面的 Sway 配置，也不向用户的 systemd 环境写变量。窗口使用软件渲染，日志里的 GPU 加速回退提示不等于启动失败。

日志保存在 `~/.local/state/sidera-wayland-lab/<日期时间-进程号>/`：

- `desktop.log`：测试桌面与面板输出。
- `sidera-wayland-*.log` / `sidera-x11-*.log`：每次启动的 Sidera 后端与绘图日志。
- `home/`：本次测试的独立配置；关闭后保留以便排查，下次创建新目录。

本工具用于覆盖层、鼠标穿透和缩放测试。它不是完整 WPS 办公桌面，也不证明物理触摸、多屏、截图/剪贴板、全局热键和 WPS 联动已通过。

## 自动回归

仓库根目录运行，使用新的临时用户目录及 headless Sway，不连接当前图形桌面：

```sh
sudo apt install --no-install-recommends build-essential libwayland-dev libwayland-bin gir1.2-gdkpixbuf-2.0
python3 rust/tests/test_wayland_native.py /absolute/path/to/sidera \
  --unit-binary /absolute/path/to/rust-test-binary
```

测试程序可由 `cd rust && cargo test --locked --no-run` 生成，Cargo 会输出路径。`--unit-binary` 启用两项真实 wl_shm 存储测试（模拟延迟 release）；省略时只执行桌面交互回归。鼠标辅助程序从 `wayland_pointer.c` 和仓库协议 XML 编译，使用持续存在的 Wayland 虚拟鼠标；不使用 XTest。也可通过 `--pointer-helper` 指定在同架构环境编译好的辅助程序。

自动场景关闭 XWayland，并覆盖首次 1920×1080 configure、连续画线、完整 press/release 穿透、切换前后笔迹像素比较、分辨率变化及 100% / 125% / 150% / 200% 缩放。通过结果只证明这里列出的场景。

实现所依据的接口：[Wayland 共享缓冲规范](https://wayland.freedesktop.org/docs/html/apa.html)、[Sway 1.11 配置参考](https://github.com/swaywm/sway/blob/1.11/sway/sway.5.scd)。
