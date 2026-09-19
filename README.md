**Sidera (星罗) — Linux 屏幕批注与教学演示工具**  
**专为 Linux 教学与演示设计的免费、轻量级屏幕批注工具，深度集成 WPS 演示联动**  
**(** **新手** **第一次vibecoding，** **还请** **多多指教** **)**  
[![](data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAABmJLR0QA/wD/AP+gvaeTAAAACXBIWXMAAA7EAAAOxAGVKw4bAAAADUlEQVR4nGP4//8/AwAI/AL+p5qgoAAAAABJRU5ErkJggg==)  
](https://github.com/offduckling4825/Sidera--screen-annotate-for-linux/actions/workflows/ci.yml "https://github.com/offduckling4825/Sidera--screen-annotate-for-linux/actions/workflows/ci.yml")  
   
 [![](data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAHAAAAAUCAYAAABVhbkHAAAABmJLR0QA/wD/AP+gvaeTAAAACXBIWXMAAA7EAAAOxAGVKw4bAAAA0UlEQVR4nO3ZQQqCUBDG8e9NtngIiofoSK7dtauV1+gO7w4dLBLCVU4LDaI0QovXwPfD1TyUgT+60ZVlufbeHwBUADIMVBVjpuZTZ996zi/mx832+eR+jd30Oh+eMzV/d7Zw3sBJQFrUyRBvN7Yz/a0M3XWPywmC/s0ji7SrBA+fTTJGNZfYO9AyDGgcAxrHgMYxoHEMaBwDGseAxjGgcQxoHAMax4DGMaBxAqCJvQTN5NxZAITYe9AsCichadu29t4D/Y/dPPJS9JkzZBWQFvUN1GlM6iw5UF0AAAAASUVORK5CYII=)  
   
 ](https://en.cppreference.com/ "https://en.cppreference.com/")[![](data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAADwAAAAUCAYAAADRA14pAAAABmJLR0QA/wD/AP+gvaeTAAAACXBIWXMAAA7EAAAOxAGVKw4bAAAAq0lEQVR4nO3YsQ3CMBCF4f9MKK5xmIwudSaAKmuwg/dgqqRxhY4iSCAhASmwpZO/6mzZ1nutZRiGvapegNHMIi/MbNO89dzheH2u182P89ubP94BFoIljTJ1j7InHDOI3Djn2QjAWDtQKWaMAYhfT3ph9KF2htJaYe9aYe9aYe9aYe9aYe9aYe8CsNQOUYwwByDVzlGIiZC6nPOkqrB+BPSVQ/2FwMyOpFGmOx51SsGSD8CcAAAAAElFTkSuQmCC)  
   
 ](https://www.qt.io/ "https://www.qt.io/")[![](data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAIQAAAAUCAYAAABMIpXkAAAABmJLR0QA/wD/AP+gvaeTAAAACXBIWXMAAA7EAAAOxAGVKw4bAAAA40lEQVR4nO3aIQ7CQBCF4X83IBYBBsW9qjkBKK6A5A69Gh4DggoCgyhNGKBFELINvM+smM52snlpzYaiKIYppQ0wB8bcMTO6dNXbat/Ys6segPVs2zxQLy+bm+VF1Vor3X2f9L7ta33jm97nMwhwiIFyOhquBrcwLNr2lt9nMD4by93xRKT+MohwMeaRh9+E/C8zJjH3ENIvCoQ4CoQ4CoQ4CoQ4CoQ4CoQ4CoQ4CoQ4CoQ4CoQ4CoQ4CoQ4ETjkHkL6IQT2EShzDyK9YDFQDqqqWqWUoL4oM8k8lGQQYN9cobsCBy1PNJUqljoAAAAASUVORK5CYII=)  
](https://www.x.org/ "https://www.x.org/")  
![](data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAnEAAAACCAYAAAA3pIp+AAAABmJLR0QA/wD/AP+gvaeTAAAACXBIWXMAAA7EAAAOxAGVKw4bAAAANUlEQVR4nO3OMQ2AABAAsSNBCUpfEJ5YGBDBgAU2QtIq6DIzW7UHAMBfHGt1V+fXEwAAXrseHDYF+yOk59sAAAAASUVORK5CYII=)  
**🌟 核心特性**  
- 🖊️ **轻量流畅的屏幕批注**：基于 C++17 与 Qt5 构建，低 CPU/内存占用，支持自由画笔、橡皮擦、白板与 10 色色盘；笔迹随落笔时长由细到粗。  
- 🖱️ **智能点击穿透（XShape 遮罩）**：在光标模式下只响应侧边栏与弹窗交互，全屏透明区域完全穿透，不影响底层桌面与课件操作。  
- 🧑🏫 **白板模式**：一键把透明批注层变为不透明白板（背景可切换墨绿/白），翻页与笔迹完全本地、与外部放映隔离；退出即恢复透明联动。  
- ✋ **手掌/大触点橡皮**：多点或大面积触点（如手背）自动临时当大号橡皮，单点恢复画笔；触发阈值与橡皮倍率可调。  
- ↩️ **撤回**：一键撤回最近一笔（最多 12 步）。  
- 📸 **截图**：一键抓屏保存 PNG 到图片目录并复制到剪贴板。  
- 📊 **WPS 演示深度联动**：  
  - 内置本地 HTTP 桥接服务（127.0.0.1:16666）与专属 JS 加载项。  
  - 支持侧边栏直接控制幻灯片翻页。  
  - **批注分页缓存**：仅在真实换页时自动切换批注缓存，页内动画触发时不丢失笔迹。  
- 🖥️ **跨架构支持**：原生支持 x86_64 (amd64) 与 aarch64 (arm64) 架构，适配国产化 Linux 教室机。  
![](data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAnEAAAACCAYAAAA3pIp+AAAABmJLR0QA/wD/AP+gvaeTAAAACXBIWXMAAA7EAAAOxAGVKw4bAAAANklEQVR4nO3OQQmAABRAsScYxpg/h5VMYARvRrCCNxG2BFtmZquOAAD4i3Ot7mr/egIAwGvXA224BcUMk6pDAAAAAElFTkSuQmCC)  
**📦 安装与快速开始**  
**方式一：安装预编译 DEB 包（推荐）**  
教室机或 Debian / Ubuntu 系发行版可直接安装对应的 release 包：  
# amd64 (x86_64)  
 sudo dpkg -i sidera_2.6-Geo-stable_amd64.deb  
   
 # aarch64 (arm64)  
 sudo dpkg -i sidera_2.6-Geo-stable_arm64.deb  
   
**方式二：从源码编译**  
***1. 安装编译依赖***  
**Ubuntu / Debian / Deepin / UOS：**  
sudo apt update  
 sudo apt install -y build-essential pkg-config \  
     qtbase5-dev libqt5widgets5 libqt5network5 \  
     libx11-dev libxcb1-dev libxext-dev libxtst-dev  
   
**Arch Linux：**  
sudo pacman -S --needed base-devel qt5-base libx11 libxcb libxext libxtst  
   
***2. 编译***  
# 编译 amd64 二进制  
 make -j$(nproc)  
   
 # 运行  
 ./annotate_amd64  
   
![](data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAnEAAAACCAYAAAA3pIp+AAAABmJLR0QA/wD/AP+gvaeTAAAACXBIWXMAAA7EAAAOxAGVKw4bAAAANUlEQVR4nO3OQQmAABRAsSd49m4v6wg/pwmMYQVvImwJtszMXp0BAPAX91pt1fH1BACA164Hoq8EQMMPmF8AAAAASUVORK5CYII=)  
**🎓 教室与教学使用流程**  
1. **启动 Sidera**：登录系统后自动或手动启动 Sidera（可在设置中勾选「开机自启动」）。  
2. **打开 WPS 演示**：打开课件，WPS 功能区会自动加载「批注联动」标签。  
3. **F5 放映授课**：放映过程中使用 Sidera 侧边栏的 ▲/▼ 按钮或画笔工具进行课件讲解与板书。  
*更多教室部署细节请参考 *[ *README.classroom.md* *。*](README.classroom.md "README.classroom.md")  
![](data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAnEAAAACCAYAAAA3pIp+AAAABmJLR0QA/wD/AP+gvaeTAAAACXBIWXMAAA7EAAAOxAGVKw4bAAAANklEQVR4nO3OQQmAABRAsSeYxZw/lieLGMACBrCCNxG2BFtmZquOAAD4i3Ot7mr/egIAwGvXA6fGBdgoVMwYAAAAAElFTkSuQmCC)  
**🛠️ 项目架构**  
src/  
 ├── main.cpp        # 程序入口、事件循环、全局热键管理  
 ├── app.h           # 全局状态单例与结构体定义  
 ├── widget.h/cpp    # 主画布与全屏覆盖层窗口  
 ├── sidebar.cpp     # 侧边悬浮工具栏与按钮交互  
 ├── drawing.cpp     # 绘图引擎、笔触渲染与撤回逻辑  
 ├── modes.cpp       # 模式切换（画笔/光标）与 XShape 输入穿透区域计算  
 ├── wps.cpp         # WPS 加载项本地 HTTP 服务与联动逻辑  
 ├── popups.cpp      # 色盘与参数调整浮窗  
 ├── splash.cpp      # 启动欢迎与加载引导界面  
 └── settings.cpp    # 设置窗口与系统环境诊断工具  
   
![](data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAnEAAAACCAYAAAA3pIp+AAAABmJLR0QA/wD/AP+gvaeTAAAACXBIWXMAAA7EAAAOxAGVKw4bAAAANUlEQVR4nO3OYQ1AABSAwY8JoIGqr4Z6Eoiggn9mu0twy8wc1RkAAH9xbdVa7V9PAAB47X4A9C4EIsmYmgsAAAAASUVORK5CYII=)  
**🧭 路线规划与 Wayland 适配说明 (Roadmap)**  
当前版本原生针对 X11 显示服务进行了底层优化。针对社区关注的 **Wayland 适配**，建议演进路径如下：  
- **WPS 联动解耦**：将当前的按键模拟（XTest）与窗口轮询（XQueryTree）全面升级为基于 WPS JS Add-in 的双向 HTTP/WebSocket 事件通知。  
- **输入区域与置顶抽象**：在 Wayland 环境下适配 wlr-layer-shell（Overlay 层）与 QWidget::setMask。  
- **全局快捷键迁移**：接入现代 Linux org.freedesktop.portal.GlobalShortcuts 协议。  
![](data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAnEAAAACCAYAAAA3pIp+AAAABmJLR0QA/wD/AP+gvaeTAAAACXBIWXMAAA7EAAAOxAGVKw4bAAAANUlEQVR4nO3OQQmAABRAsSd4NIGRTPXNaQBrWMGbCFuCLTOzV2cAAPzFvVZbdXw9AQDgtesBhZQEOYZGgUEAAAAASUVORK5CYII=)  
**🤝 贡献指南**  
欢迎提交 Issue 和 Pull Request！  
- 提交 PR 前请确保本地编译通过：make clean && make  
- 建议保持轻量无冗余依赖的设计理念。  
![](data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAnEAAAACCAYAAAA3pIp+AAAABmJLR0QA/wD/AP+gvaeTAAAACXBIWXMAAA7EAAAOxAGVKw4bAAAANklEQVR4nO3OQQmAABRAsSfYxZo/jzlMYQLPJrCCNxG2BFtmZquOAAD4i3Ot7mr/egIAwGvXA4q7Bc870TqdAAAAAElFTkSuQmCC)  
**📄 许可证**  
本项目遵循开源许可协议，详见 [LICENSE 文件。](LICENSE "LICENSE")  
