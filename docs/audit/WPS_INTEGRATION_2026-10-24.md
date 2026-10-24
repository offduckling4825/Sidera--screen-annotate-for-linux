# WPS 联动与翻页授权修复（2026-10-24）

后续行为更新：用户要求恢复未接入时模拟键盘，并新增待放映启动按钮。当前实现已改用免弹窗键盘能力回退，XWayland 使用 uinput；详细行为和新版验收边界见 [触摸界面与 WPS 改进](TOUCH_UI_2026-10-24.md)。下文保留 `.4` 修复过程与人工确认记录，其中“联动开启时不回退”是当时策略。

`.8` 按用户要求将启动按钮改为从当前查看页开始，使用 WPS 内置“当页开始”命令；详见 [后续记录](WPS_CURRENT_SLIDE_2026-10-24.md)。

## 范围与当前结论

分支 `custom`，Rust 版；保留前面已验证的点击穿透和原生 Wayland 修复。
用户要求联动异常时只改变按钮颜色，且由用户操作 WPS 做最终验收。
在收到停止测试的指示后，停止独立桌面的交互测试；不把模拟接口的测试结果当作真实 WPS 通过。

## 查到的问题

- VM 中本项目的 WPS 登记是 `enable_dev`；旧 Rust 自动登记只检查名称存在，没有修正其属性。
- 联动断开时，上下翻页自动回退到 XTEST 模拟键盘。当前 GNOME 的 Xwayland 带 `-enable-ei-portal`，这条路径可能请求 RemoteDesktop 授权。
- 静态文件和启动问候也能把桥标记在线，退出放映、重启桥和延迟命令的状态不完整。
- VM 的 WPS OEM 配置没有显式开启官方的 `JsApiPlugin` 开关，独立桌面里尚未观察到加载项接入。

## 已实现

- Rust 自动登记复用已有的安全 XML 合并脚本，修正 `enable_dev`，保留其他加载项、备份后原子替换。
- 开启 WPS 联动时，上下翻页与退出放映只走 WPS API；未连接或尚未放映时不猜测页号、不发送模拟键盘。
- 显式关闭联动后，仅原生 X11 或具备直接注入能力的 Wayland 后端保留原来的按键路径；Xwayland 不隐式请求输入授权。
- 上下翻页按钮未就绪时使用橙色，正常时恢复原色；白板保持原行为。不增加文字提示或授权窗口。
- 加载项支持 `NEXT`、`PREV`、`EXIT` 和失败反馈；优先使用现有 Application，也兼容 `wps.WppApplication()`。
- 轮询带实际放映快照，限制为一个逻辑在途请求；迟到回复不执行。服务器限制指令队列为 8 条、有效期 1.5 秒，在断线、退出放映、进入白板时清掉待执行命令。
- 静态请求不再算在线；同页动画不切批注页；联动开启时不再用全屏窗口猜测结果清空批注。
- 新增 `wps-addin/enable-jsapi.py`：按 WPS 官方文档开启 `[Support] JsApiPlugin=true`，备份并保留其余 OEM 配置。不启用网页调试器，不改服务器地址。

## 用户要求停止测试之前的结果

- Rust：35 项通过，13 项图形协议测试未在本轮运行。
- 加载项 JavaScript：27 项通过。
- 安全登记脚本：13 项通过。
- 启动检查：16 项通过（独立网络和临时目录）；首次未隔离 WSL 的 X11 socket 目录时环境启动失败，隔离后通过。
- release 构建完成。上述结果不证明当前 WPS 已实际执行翻页。

独立 GNOME 桌面中只打开了生成的三页测试课件，未完成真实 WPS 翻页、退出、动画和笔迹恢复验收。
官方加载项开关的实际效果也留给用户重启 WPS 后确认。

## 安装记录

已覆盖安装 `3.0-Electro-testing+custom20261024.4`，随后以用户身份合并加载项登记。
本次本地 amd64 包要求 glibc 2.43，仅用于当前 Ubuntu 26.04 VM。

- DEB SHA256：`84ab8e8a983896226492730c4e6dbb0dfe4229bf6e0a1bf998699e1e1819c8ca`
- 已安装程序 SHA256：`fc6c5a89a26ff8c7ca7f2a1e9665f431517967a7e2b48ad850c17c7ced66b316`
- WPS OEM 原文件备份：`/opt/kingsoft/wps-office/office6/cfgs/oem.ini.sidera-backup-k5a6621r`
- 上一程序备份：`/var/backups/sidera-compat-20261024/sidera-before-wps-custom4`；此前 `.3` 安装包也保留在宿主机。
- 安装日志：VM `/var/tmp/sidera-wps-install.log`。

OEM 开关是此次 VM 部署时显式运行辅助脚本设置的；普通 DEB 安装不会擅自修改其他机器的 WPS 配置。

## 用户验收

**后续反馈：用户已明确回复“成功了”。** 记录为 `.4` 在当前 VM 上的 WPS 联动人工确认；未单独反馈的同页动画、复杂课件、多屏与所有生命周期场景不自动标为通过。用户要求继续由其自行进行图形交互测试。

在普通 Ubuntu 桌面退出 Sidera 和全部 WPS 窗口；先打开 Sidera，再打开 WPS 演示并放映课件。
确认上下翻页恢复原色，前后翻页不弹共享桌面授权，返回上一页后对应笔迹仍在；最后确认退出放映。
若保持橙色，应检查 WPS 是否出现“批注联动”选项卡及 `~/wps-api-debug.log`，不能把“没有授权窗口但也没有翻页”算通过。

VM：Ubuntu 26.04.1 amd64，GNOME 50.1 Wayland；WPS `12.1.2.29183.AK.preread.sw`。
本轮没有验收麒麟 ARM64，也没有修改 C++ 桥的按键回退逻辑。
多客户端身份、命令 ID/完整执行确认、事件乱序与 HTTP 边界仍由 S10/S12 跟踪。

## 参考

- [WPS 加载项开发说明](https://open.wps.cn/documents/app-integration-dev/wps365/client/wpsoffice/wps-integration-mode/wps-addin-development/wps-addin-development-instructions)
- [WPS 加载项可用性](https://open.wps.cn/documents/app-integration-dev/wps365/client/wpsoffice/jsapi/addin-api/wps-addin-availability)
- [WPS SlideShowWindow](https://open.wps.cn/documents/app-integration-dev/wps365/client/wpsoffice/jsapi/wpp/SlideShowWindow/obj)
