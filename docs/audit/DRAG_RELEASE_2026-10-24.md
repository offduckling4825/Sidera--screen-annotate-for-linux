# 工具栏鼠标拖动释放修正

> 历史阶段记录：以下版本、安装状态、测试限制与“未提交”等表述仅指当时。当前代码和隔离检查见 [最新复查](REVIEW_2026-10-24.md)，当前使用规则见 [教室指南](../../README.classroom.md)。

日期：2026-10-24。分支 `custom`。用户确认问题发生在鼠标拖动 Sidera 两侧工具栏，松开后仍继续移动。遵循用户自行操作测试的要求，本轮只读日志、检查源码、编译、打包和安装。

## 依据与修正

只读日志确认当前 Sidera PID 13008 从普通入口启动，后端为 `GNOME 原生 Wayland`；扩展版本 3 / ACTIVE，Shell 没有匹配的异常日志。该进程仍使用升级前的同版主程序映像；没有重启或终止它。日志没有逐个输入事件，不能仅据此确定用户这次 Release 丢失的具体位置。

源码确认两条需要修正的状态路径：

1. 工具栏背景原来接受任意鼠标按键开始拖动，但拖动期间 `on_release` 只接受左键；右键或滚轮按下因此可能建立无法由同一按钮释放的拖动。现在背景只有左键可以开始拖动。
2. 拖动期间新的按下原来仍能进入按钮等待状态，随后松手优先结束该按钮而留下拖动。现在拖动在收到释放或取消前保留归属，不接受新的按下覆盖。

GNOME 扩展版本 4 另补漏送 Release 的恢复：真实 Motion 事件携带当前按键位掩码，若此前记录的左键已经松开，在发送该次移动前先向 Rust 发送取消，停止拖动并刷新输入区。取消不触发当前位置的按钮；活动触摸不会被物理鼠标的状态核对取消。其他已经松开的按键也从扩展记录中移除，避免残留记录阻止更新输入区。

不使用闲置超时；静止但仍按住的鼠标不会被误释放。输入 actor 的正常保留与延迟更新机制不变，不重映射视觉层，也没有改工具栏位置和外观。对照 GNOME 50.1 安装源码及 Mutter 50.1 的 `clutter-sprite.c`，Clutter 已提供隐式抓取；本次没有叠加全局输入抓取。

## 检查与验收边界

- `cargo check --tests --locked` 通过；新增回归源覆盖非左键不拖动、拖动期间额外按下不抢占、右键松开不终止左键拖动、左键松开后移动不再改变位置。只编译，未执行。
- 扩展通过 `node --check`，未执行 GNOME 模块或输入自动化。
- 20 个原布局/命中函数源码未变，构建缓存与工作区一致。
- 没有操作当前工具栏、发送试验按键或替用户退出应用。真实鼠标拖出原位置、屏幕边缘释放、多指/触控及兼容模式仍由用户验收。

用户验收：拖动两侧工具栏向上/下移动，在原工具栏范围外松开，再移动鼠标，确认工具栏停留；同时检查普通单击、滑出取消与画笔/光标切换。原生模式需新扩展加载后再验收。

## 构建与部署

`cargo build --release --locked` 通过。VM 已覆盖安装 `3.0-Electro-testing+custom20261024.9`，dpkg 为 `install ok installed`。普通用户安装器已更新用户扩展文件到版本 4，schema 编译成功，文件校验和与系统资源一致；现有 WPS“当页开始”脚本保留 `.8` 的内容。

安装后只读查询：Shell PID 11860 仍缓存扩展版本 3 / ACTIVE，Sidera PID 13008 仍在运行旧程序。已请用户保存内容后注销、重新登录加载版本 4；尚未确认新模块加载或拖动效果，未替用户结束当前进程。

后续 `.11` 安装后只读确认：新 Shell PID 17662 已加载扩展版本 4 / ACTIVE。首次加载步骤已完成，鼠标实际拖动结果仍待用户反馈，未执行交互测试。

| 制品 | SHA256 |
| --- | --- |
| `.9` amd64 DEB | `427f03b14699407d6c89b63642e3aabaf434a80f8299604913f41abe6adf2be7` |
| `/usr/bin/sidera` | `8b395ad6498b5ae14ff4848576dcec4b63fffac1d9498a75a35029aa1d550c00` |
| 系统和用户 `extension.js` | `97506baee83dff7938fb1346b4da2b6a1a2398bb9ddea7dbc2c03db1c5ab0000` |
| WPS `js/bridge.js` | `a666696c7640565d0db48d005360214e247760df3ff2193d4d86e8fd0d639462` |

打包记录：WSL `/var/tmp/sidera-install-d8d04c7-68421ff3/drag-release9-build.log`、`drag-release9-package.log`。VM 安装记录：`/var/tmp/sidera-drag9-install.log`、`sidera-drag9-user-install.log`。本地 amd64 包声明 `libc6 >= 2.43`，仅供当前 Ubuntu 26.04.1 VM 使用。

回退：`/var/backups/sidera-compat-20261024/` 保存 `.8` 包和 `sidera-before-drag9`；旧用户扩展保存在 `/home/shi01/.local/share/sidera/extension-backups/20261024-155656-f3a40ad7`。未提交或 push。

诊断记录：VM `/var/tmp/sidera-drag-investigate.log`、`sidera-drag-backend.log`。读取的 Shell `dnd.js` / `slider.js` 仅作接口对照；没有执行这些源文件。

上游源码依据：[Mutter 50.1 ClutterSprite](https://github.com/GNOME/mutter/blob/50.1/clutter/clutter/clutter-sprite.c)。
