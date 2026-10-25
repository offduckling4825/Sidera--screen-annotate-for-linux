# README 界面预览

这些 PNG 由 `rust/examples/render_ui.rs` 和 `rust/examples/render_themes.rs` 调用生产界面绘制函数生成，2026-10-25 随三档主题更新，与 custom.34 的绘制实现一致。它们不是桌面验收截图：课堂背景、WPS 页码及准备状态由预览程序设置，动画图是离散分帧。

两张课堂图模拟的放映动作不同：浅色图显示可从当前页开始放映，深色图显示 Esc；两图的上下翻页均处于可用状态。Esc 也可出现在联动在线但没有可用开始/结束动作的场景，不能据此判断未联动。背景是模拟课件，不代表白板底色跟随主题变化。

| 图片 | 用途 |
| --- | --- |
| classroom.png | 浅色课堂场景、双侧工具栏与画笔面板 |
| classroom-dark.png | 深色课堂场景，批注色块保留原色 |
| themes-dark.png | 深色设置、启动、工具面板、页码拨轮、诊断与退出确认 |
| toolbar-dark.png | 深色选中/禁用、拖动/按下、绘图组折叠和贴边收起 |
| startup.png | 启动状态与主次署名 |
| settings.png | 设置、参数与恢复默认 |
| page-wheel.png | 页码拨轮的不同状态 |
| startup-reveal.png | 工具栏由中心向上下展开的分帧 |

在 Linux 的 rust/ 目录可运行：

```sh
cargo run --locked --example render_ui -- /tmp/sidera-preview /path/to/CJK-font.ttc
cargo run --locked --example render_themes -- /tmp/sidera-theme-preview
```

该程序仅生成图像，不连接桌面后端，也不能验证真实输入、帧率或兼容性。图标和源代码的归属及许可见根目录 [NOTICE](../../../NOTICE.md) 和 [LICENSES](../../../LICENSES.md)。
