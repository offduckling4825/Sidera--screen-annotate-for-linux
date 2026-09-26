# Sidera（星罗）— Linux 屏幕批注与教学演示工具

**专为 Linux 教学与演示设计的免费、轻量级屏幕批注工具，深度集成 WPS 演示联动**

**(新手第一次 vibecoding，还请多多指教)**

[![CI](https://github.com/offduckling4825/Sidera--screen-annotate-for-linux/actions/workflows/ci.yml/badge.svg)](https://github.com/offduckling4825/Sidera--screen-annotate-for-linux/actions/workflows/ci.yml)
[![C++17](https://img.shields.io/badge/C%2B%2B-17-blue)](https://en.cppreference.com/)
[![Rust](https://img.shields.io/badge/Rust-stable-orange)](https://www.rust-lang.org/)
[![Qt5](https://img.shields.io/badge/Qt-5-green)](https://www.qt.io/)
[![X11](https://img.shields.io/badge/X11-supported-lightgrey)](https://www.x.org/)
[![Wayland](https://img.shields.io/badge/Wayland-layer--shell-blueviolet)](#)

---

## 🧬 两套并行实现

本仓库包含**两套并行维护、均可用于生产**的实现，按平台与依赖偏好选择：

| 版本 | 技术栈 | 版本号 | 建议场景 |
| --- | --- | --- | --- |
| **Rust 版（主推）** | 纯 Rust，**不依赖 Qt / libX11**，tiny-skia 软件渲染 | `3.0-Electro-testing` | X11 与 Wayland 通吃；老机器、国产化教室机；追求轻量与无重量级依赖 |
| **C++ / Qt5 版** | C++17 + Qt5 | `Electro-testing` | 经典实现；长期在 X11 教室环境验证，功能最全 |

> 两版共用同一份 WPS 加载项（`wps-addin/`）与本地 HTTP 桥协议（`127.0.0.1:16666`），可平滑切换。

---

## 🌟 核心特性

- 🖊️ **轻量流畅的屏幕批注**：自由画笔、橡皮擦、白板与 10 色色盘；笔迹随落笔时长由细到粗。
- 🖱️ **智能点击穿透**：光标模式下只响应侧边栏与弹窗交互，全屏透明区域完全穿透，不影响底层桌面与课件操作。
- 🧑‍🏫 **白板模式**：一键把透明批注层变为不透明白板（背景可切换墨绿/白），翻页与笔迹完全本地、与外部放映隔离；退出即恢复透明联动。
- ✋ **手掌/大触点橡皮**：多点或大面积触点（如手背）自动临时当大号橡皮，单点恢复画笔；触发阈值与橡皮倍率可调。
- ↩️ **撤回**：一键撤回最近一笔（最多 12 步）。
- 📸 **截图**：一键抓屏保存 PNG 到图片目录并复制到剪贴板。
- 📊 **WPS 演示深度联动**：
  - 内置本地 HTTP 桥接服务（127.0.0.1:16666）与专属 JS 加载项。
  - 支持侧边栏直接控制幻灯片翻页。
  - **批注分页缓存**：仅在真实换页时自动切换批注缓存，页内动画触发时不丢失笔迹。
- 🖥️ **跨架构支持**：原生支持 x86_64 (amd64) 与 aarch64 (arm64)，适配国产化 Linux 教室机。

---

## 📦 安装与快速开始

### 方式一：安装预编译 DEB 包（推荐）

> 两种实现共用 `sidera` 包名，**二选一安装**。

**Rust 版（主推）：**

```bash
# amd64 (x86_64)
sudo dpkg -i sidera_3.0-Electro-testing_amd64.deb

# aarch64 (arm64)
sudo dpkg -i sidera_3.0-Electro-testing_arm64.deb
```

**C++ / Qt5 版：**

```bash
# amd64 (x86_64)
sudo dpkg -i sidera_Electro-testing_amd64.deb

# aarch64 (arm64)
sudo dpkg -i sidera_Electro-testing_arm64.deb
```

### 方式二：从源码编译

#### Rust 版

Rust 版运行时只依赖 `libc6` 与 `libgcc-s1`，无需 Qt、无需 `libX11`（X11/Wayland 协议均由纯 Rust 库直连）。

```bash
cd rust

# 本机编译（开发用）
cargo build --release
./target/release/sidera

# 运行测试
cargo test
```

面向老机器（更低 glibc 依赖）的容器交叉编译与打包（需要 `bwrap` + rustup 工具链）：

```bash
cd rust
./build_container.sh amd64     # 产物: target/container-amd64/release/sidera
./build_deb.sh amd64           # 产物: ../sidera_3.0-Electro-testing_amd64.deb
```

aarch64 / arm64 一键编译 + 打包（需宿主机 `bwrap`、`qemu-aarch64-static` 与对应 Rust 工具链）：

```bash
cd rust
./build_aarch64.sh             # 编译 + 打包 arm64 DEB
./build_aarch64.sh --no-deb    # 只编译，不打包
```

> 也可手动指定架构：`./build_container.sh aarch64 && ./build_deb.sh aarch64`。

#### C++ / Qt5 版

**1. 安装编译依赖**

Ubuntu / Debian / Deepin / UOS：

```bash
sudo apt update
sudo apt install -y build-essential pkg-config \
    qtbase5-dev libqt5widgets5 libqt5network5 \
    libx11-dev libxcb1-dev libxext-dev libxtst-dev
```

Arch Linux：

```bash
sudo pacman -S --needed base-devel qt5-base libx11 libxcb libxext libxtst
```

**2. 编译**

```bash
cd cpp

# 本机编译 amd64 二进制
make -j$(nproc)

# 运行
./annotate_amd64

# 或用仓库内容器交叉编译（低 glibc 依赖）
./build_with_container.sh amd64     # 产出 cpp/annotate_amd64
```

> 打包 DEB：`cd cpp && ./build_deb_amd64.sh`（产物 `cpp/sidera_Electro-testing_amd64.deb`）。

---

## 🎓 教室与教学使用流程

1. **启动 Sidera**：登录系统后自动或手动启动 Sidera（可在设置中勾选「开机自启动」）。
2. **打开 WPS 演示**：打开课件，WPS 功能区会自动加载「批注联动」标签。
3. **F5 放映授课**：放映过程中使用 Sidera 侧边栏的 ▲/▼ 按钮或画笔工具进行课件讲解与板书。

*更多教室部署细节请参考 [README.classroom.md](README.classroom.md)。*

---

## 🛠️ 项目架构

### 仓库结构

```
.
├── cpp/          # C++ / Qt5 版：Makefile、src/、tests/、打包/容器脚本
├── rust/         # Rust 版：Cargo 工程、build_container.sh / build_aarch64.sh / build_deb.sh
├── protocols/    # Wayland 协议 XML（两版共用）
├── wps-addin/    # WPS JS 加载项（两版共用）
├── README.md
└── LICENSE
```

### Rust 版（`rust/`）

```
rust/src/
├── main.rs        # 程序入口、事件分发、全局热键、触摸/输入区域路由
├── app.rs         # 全局状态单例、配置持久化、多页缓存
├── ui.rs          # 手绘 UI（侧边栏/弹窗/设置）与命中测试、区域渲染
├── paint.rs       # 笔迹/橡皮光栅（tiny-skia），并集路径防毛边
├── gesture.rs     # 手掌/大触点橡皮手势状态机
├── x11.rs         # X11 后端：ARGB 覆盖层、XShape 穿透、XTest、热键、截图
├── wayland.rs     # Wayland 后端：wlr-layer-shell、输入区域、虚拟键盘
├── backend.rs     # 后端抽象（X11 / Wayland 共用上层逻辑）
├── wps.rs         # WPS 本地 HTTP 桥（127.0.0.1:16666）与加载项注册
├── portal.rs      # xdg-desktop-portal 截图（无 screencopy 的合成器）
├── uinput.rs      # 内核虚拟键盘注入（KWin 等无虚拟键盘协议时）
├── text.rs        # 字体栅格化（ab_glyph → tiny-skia 路径）
└── splash.rs      # 启动闪屏

rust/src/keymap.xkb  # 内嵌 xkb keymap（虚拟键盘用）
```

Wayland 协议 XML 位于仓库根 `protocols/`（wlr-layer-shell、xdg-shell、viewporter、virtual-keyboard 等）。

### C++ / Qt5 版（`cpp/src/`）

```
cpp/src/
├── main.cpp        # 程序入口、事件循环、全局热键管理
├── app.h           # 全局状态单例与结构体定义
├── widget.h/cpp    # 主画布与全屏覆盖层窗口
├── sidebar.cpp     # 侧边悬浮工具栏与按钮交互
├── drawing.cpp     # 绘图引擎、笔触渲染与撤回逻辑
├── modes.cpp       # 模式切换（画笔/光标）与 XShape 输入穿透区域计算
├── wps.cpp         # WPS 加载项本地 HTTP 服务与联动逻辑
├── wps_bridge.cpp  # Wayland 后端下的 WPS 桥实现
├── popups.cpp      # 色盘与参数调整浮窗
├── splash.cpp      # 启动欢迎与加载引导界面
├── settings.cpp    # 设置窗口与系统环境诊断工具
├── backend_wayland.cpp  # Wayland 后端（wlr-layer-shell）
└── ...
```

---

## 🧭 路线规划与 Wayland 适配说明 (Roadmap)

- **Rust 版已原生支持 Wayland**：通过 `wlr-layer-shell`（niri/sway/hyprland 等）实现 Overlay 层置顶与 `wl_surface` 输入区域；无 `zwp_virtual_keyboard` 的合成器自动回退到 `uinput` 内核虚拟键盘。
- **X11 兼容**：Rust 版在 X11 下使用 `x11rb` 直连（ARGB 覆盖层 + XShape 输入穿透 + XTest 注入），不依赖 `libX11`/`libXtst`。
- **截图后端回退**：Wayland 无 `wlr-screencopy` 时走 `xdg-desktop-portal`。
- **WPS 联动解耦**：持续将按键模拟/窗口轮询迁移为基于加载项的双向 HTTP/WebSocket 事件通知。
- **全局快捷键迁移**：接入现代 Linux `org.freedesktop.portal.GlobalShortcuts` 协议。

---

## 🤝 贡献指南

欢迎提交 Issue 和 Pull Request！

- C++ 版提交 PR 前请确保本地编译通过：`make clean && make`
- Rust 版提交 PR 前请确保通过：`cd rust && cargo test && cargo clippy --all-targets`
- 建议保持轻量无冗余依赖的设计理念。

---

## 📄 许可证

本项目遵循开源许可协议，详见 [LICENSE 文件](LICENSE)。
