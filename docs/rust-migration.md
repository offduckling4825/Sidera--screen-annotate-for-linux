# Sidera Rust 重构路线

## 当前基线

上游 `main` 是 C++17 + Qt5 + X11 实现，功能已经能工作；本分支不把原实现删除，而是用可验证的增量迁移降低回归风险。

已确认的原版能力：

- 透明全屏批注层，光标模式下侧边栏和弹窗以外的区域穿透到底层桌面。
- 画笔、橡皮、颜色和粗细选择、最多 12 步撤回、截图到图片目录并复制到剪贴板。
- 白板模式，独立白板页缓存，墨绿/白背景切换，最多 10 页。
- WPS 演示翻页按钮、退出放映、真实页号驱动的批注分页缓存。
- 手背大触点或多指触摸触发临时大号橡皮。
- `127.0.0.1:16666` WPS HTTP bridge：`/hello`、`/push`、`/poll` 和加载项静态文件。
- X11 全局快捷键、XShape 输入穿透和 XTest 按键/鼠标注入。

## Milestones

### M0：可编译的 Rust 基础层

- [x] Cargo workspace。
- [x] `sidera-core`：模式、笔画数据模型、撤回历史、多页缓存、WPS 事件解析。
- [x] `sidera-bridge`：无第三方运行时依赖的本地 HTTP bridge 和静态加载项服务。
- [x] bridge 按 3 秒无请求判定 WPS 加载项离线，并清除旧的真实页号。
- [x] `sidera` CLI：`help`、`diagnose`、`serve`。
- [x] core/bridge 单元测试与 GitHub Actions 检查。
- [x] Rust GUI overlay：X11/Wayland 后端、绘制、输入、侧栏、弹窗、白板和 WPS 联动已进入 `rust/src`。

### M1：Rust bridge 替换与真实联调

- 将 Rust bridge 接入 Rust GUI/旧 GUI 的状态边界，不能同时让两个进程抢占 16666 端口。
- 用真实 WPS 加载项验证 `SlideShowBegin`、页内动画、前进、后退和断线重连。
- 增加日志、优雅停止、单实例和配置文件迁移。

### M2：绘图与缓存渲染迁移

- 把当前 `QPixmap` 快照缓存替换为 Rust 的 stroke model + 渲染后端。
- 先迁移鼠标画笔/橡皮和撤回，再迁移触摸手势与笔迹渐粗。
- 建立像素/几何回归样例，避免只验证“能编译”。

### M3：X11/Wayland overlay 迁移

- 使用 `x11rb` 或经过验证的窗口 toolkit 实现透明置顶窗口。
- 迁移 XShape 输入区域、XTest、全局快捷键、合成器诊断。
- 在真实 X11、XWayland 和无合成器环境分别验证；Wayland 原生支持不在本阶段假装完成。

### M4：Rust GUI 与发行包

- [x] 侧边栏、弹窗、设置、截图、白板和单实例已迁移到 Rust GUI。
- 保留 Debian amd64/arm64 打包与 WPS add-in 安装路径。
- Rust GUI 通过 X11/Wayland 实机验收后，再考虑移除旧 C++ 构建路径。

## 当前阶段如何运行

```bash
cargo run -p sidera -- --help
cargo run -p sidera -- diagnose
cargo run -p sidera -- serve
```

bridge 默认只监听 `127.0.0.1:16666`。指定端口可用于不影响旧版程序的并行测试：

```bash
cargo run -p sidera -- serve --port 17666
```

workspace 中的 `sidera` GUI 已提供 X11/Wayland overlay；`serve` 子命令仍只提供 bridge 和 WPS 加载项静态资源，用于联调和协议测试。

白板缓存是一次白板会话内的临时状态：进入白板时保存当前普通/放映页，退出白板时丢弃白板笔迹并恢复原页；白板笔迹不会写入放映页缓存。
