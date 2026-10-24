# WPS 从当前页开始放映

> 历史阶段记录：以下版本、安装状态、测试限制与“未提交”等表述仅指当时。当前代码和隔离检查见 [最新复查](REVIEW_2026-10-24.md)，当前使用规则见 [教室指南](../../README.classroom.md)。

日期：2026-10-24。分支 `custom`。用户要求点击绿色“开始放映”时，从当前正在查看的幻灯片开始。本轮保留由用户操作测试的约定，没有执行测试用例、启动 WPS / Sidera 或发送试验命令。

## 行为

START 改为调用 `Application.CommandBars.ExecuteMso("SlideShowFromCurrent")`。页号由 WPS 在命令执行时确定，不使用之前准备状态里缓存的编辑页号。放映建立后仍通过现有事件和轮询上报真实位置，批注按该位置同步。

仅当前活动文档有幻灯片、没有现有放映、命令接口可用时提供启动状态；有 `GetEnabledMso` 时还检查“当页开始”是否启用，执行前重新检查。保留防重复启动与命令过期保护。命令不可用或执行失败时报告失败，不退回 `Run()` 从头启动，也不改 `StartingSlide` / `EndingSlide` / `RangeType`。

## 验证与部署

加载项脚本和改动后的回归源均通过 `node --check`；只解析语法，未执行测试。回归源覆盖命令不可用、执行前禁用、重复启动、准备后当前页变化、真实页号上报，以及命令失败不调用旧 `Run()`。

Rust 模块与 `.7` 构建源逐文件一致，复用已编译程序；GNOME 扩展仍为版本 3，无须重新安装用户扩展或注销。VM 已覆盖安装 `3.0-Electro-testing+custom20261024.8`，dpkg 为 `install ok installed`，安装后程序、WPS 脚本和 GNOME 扩展校验和已核对。本地包仍仅供当前 Ubuntu 26.04.1 amd64 / glibc 2.43 VM 使用。

| 制品 | SHA256 |
| --- | --- |
| `.8` amd64 DEB | `cc818227ff520c7f397fd85e2291131fdc0198252e8c9b676185a59754eb1e04` |
| 已安装 WPS `js/bridge.js` | `a666696c7640565d0db48d005360214e247760df3ff2193d4d86e8fd0d639462` |
| `/usr/bin/sidera`（与 `.7` 相同） | `9a25a3f522d4a39ac810cb606e15e3779beb30082501be972ec2ea016cc04c33` |

WSL 打包记录：`/var/tmp/sidera-install-d8d04c7-68421ff3/wps-current-slide8-package.log`。VM 安装记录：`/var/tmp/sidera-current8-install.log`。`.7` 包和旧脚本 `wps-bridge-before-current8.js` 备份在 `/var/backups/sidera-compat-20261024/`。未关闭或启动用户的应用，未重新登记加载项，未提交或 push。

用户验收：保存文档后退出并重新打开 WPS；在课件中选中非第一页（例如第 5 页），点击 Sidera“开始放映”，确认从该页开始，翻页与批注仍按实际页号对应。

## 接口依据

- WPS [演示 idMso 参考](https://open.wps.cn/documents/app-integration-dev/wps365/client/wpsoffice/jsapi/idmso-list/wpp-idmso-reference) 将 `SlideShowFromCurrent` 列为“当页开始”。
- WPS [CommandBars.ExecuteMso](https://open.wps.cn/documents/app-integration-dev/wps365/client/wpsoffice/jsapi/kso/CommandBars/member/ExecuteMso) 与 [GetEnabledMso](https://open.wps.cn/documents/app-integration-dev/wps365/client/wpsoffice/jsapi/kso/CommandBars/member/GetEnabledMso) 定义命令执行与可用性查询。

上述文档确认接口语义，不替代当前 Linux WPS 的实机验收。
