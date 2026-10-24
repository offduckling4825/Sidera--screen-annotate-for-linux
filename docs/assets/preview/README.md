# README 界面预览

这些 PNG 来自 `rust/examples/render_ui.rs` 对生产界面绘制函数的调用，使用 custom.29 的代码生成。它们不是桌面验收截图：课堂背景、WPS 页码及准备状态由预览程序设置，动画图是离散分帧。

| 图片 | 用途 |
| --- | --- |
| classroom.png | 浅色课堂场景、双侧工具栏与画笔面板 |
| startup.png | 启动状态与主次署名 |
| settings.png | 设置、参数与恢复默认 |
| page-wheel.png | 页码拨轮的不同状态 |
| startup-reveal.png | 工具栏由中心向上下展开的分帧 |

在 Linux 的 rust/ 目录可运行：

```sh
cargo run --locked --example render_ui -- /tmp/sidera-preview /path/to/CJK-font.ttc
```

该程序仅生成图像，不连接桌面后端，也不能验证真实输入、帧率或兼容性。图标和源代码的归属及许可见根目录 [NOTICE](../../../NOTICE.md) 和 [LICENSES](../../../LICENSES.md)。
