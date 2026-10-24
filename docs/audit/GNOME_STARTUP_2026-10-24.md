# GNOME 标准入口启动与界面修正

日期：2026-10-24。分支 `custom`。用户报告 `.6` 标准入口无法启动、仅兼容模式能用，且不接受上一版界面样式。沿用用户自行进行操作测试的要求，没有启动 Sidera / WPS、注入输入或运行测试用例。

## 诊断依据

- 当前 VMware 为 Ubuntu 26.04.1 amd64、GNOME 50.1 Wayland。扩展版本 2 状态 `ACTIVE`，本地 socket 存在；这仅证明扩展加载。
- 用户启动标准入口和 GNOME 原生入口时，当前 Shell PID 7046 的日志记录 `refreshMonitors` 未处理的异步异常。4 秒后应用提示 `Resource temporarily unavailable (os error 11)`，与 Rust 等待初始化回应的读取超时一致。
- 从 VM 的 `/usr/lib/gnome-shell/libshell-18.so` 读取实际安装的 `layout.js` 与 `params.js`：GNOME 50 的允许参数只有 `trackFullscreen`、`affectsStruts`，`Params.parse` 拒绝多余参数。原扩展仍向 `addTopChrome` 传入 `affectsInputRegion`，因此创建覆盖层会抛错。该方法先挂载 actor 再校验，原实现还可能留下部分创建的 actor。
- 同一版本的 `extensionSystem.js` 明确扩展模块只能 import 一次。覆盖磁盘文件无法更新当前 Shell 的模块，需要用户保存内容后注销、重新登录。

日志记录初始化异常，具体被移除参数由实际安装的 Shell 源码与扩展调用对照定位；本轮没有运行启动复现。VM 记录：`/var/tmp/sidera-native-startup-investigate.log`。

## 修改

- 扩展版本 3：GNOME 50 不传已移除参数；46–49 保留该参数，视觉 actor 不响应输入、透明输入 actor 响应输入的设计不变。参数构造独立到 `chrome.js`，安装器同步复制。
- 覆盖层创建失败即清理部分创建的 actor；异步初始化异常写入 Shell 日志，先回传具体错误再关闭连接，避免客户端只看到错误 11。初始化超时或中断也有明确提示。
- 显示缩放回退读取实际属性 `geometryScale`；异步显示信息读取后检查连接与请求代次，避免旧结果作用于新客户端。
- 恢复 `.6` 改版前的配色、透明未选中按钮、弹窗和启动画面，去掉新增颜色对勾。保留绿色“开始放映”、红色“结束放映”、未接入“Esc”、无悬浮和按下/松手反馈。没有移动按钮、改变布局或撤掉新触摸处理。

## 检查边界

- `cargo check --tests --locked` 通过：只编译测试源，没有执行测试。
- GNOME 扩展与新回归源 `node --check` 通过：仅语法解析，不执行 Shell 模块。
- 源码对照确认 20 个原布局/命中函数未变；这不代表视觉体验或触屏体验已验收。
- 新增 GNOME 版本参数与初始化错误回归源，留待用户允许后执行。没有连接原生 socket 做探测，因为这会实际创建覆盖层。
- 本次修改后的标准入口启动、绘图穿透、WPS、截图、多屏和触摸均未操作验收；历史兼容模式的成功不用于代替原生路径验证。

## 构建与部署

VM 已覆盖安装 `3.0-Electro-testing+custom20261024.7`，dpkg 为 `install ok installed`；程序与系统扩展文件校验和匹配构建制品。普通用户安装器已复制扩展版本 3（包括新增 `chrome.js`），设置 schema 编译成功，用户目录文件与系统资源一致。

安装后的首次只读查询仍为 Shell PID 7046、扩展版本 2 / ACTIVE。用户随后注销并重新登录；2026-10-24 06:37:45 UTC 的只读查询确认新 Shell PID 11860、扩展版本 3 / ACTIVE、已启用，桥目录权限 0700、socket 权限 0600。当前 Shell 日志中没有匹配的 Sidera / JS ERROR / 未处理 Promise 异常；查询时没有 Sidera 进程。这确认新模块已加载，不代表标准入口启动或原生交互通过。

| 制品 | SHA256 |
| --- | --- |
| `.7` amd64 DEB | `647e6dd43bf91c38d620c3c86f93566094a42aa4489791173663f62129f6dfda` |
| `/usr/bin/sidera` | `9a25a3f522d4a39ac810cb606e15e3779beb30082501be972ec2ea016cc04c33` |
| `extension.js` | `e9da14958bb261d8dea5157a903c4526a29eedf755f9f778ec249a61ca9695d4` |
| `bridge.js` | `a7b4cb525d0e0e424b3aadad4f74bc28d1bd2501a266207b71ed1408dc2e3051` |
| `chrome.js` | `6cd5970f001887e21ecc382b11340d6e99463a8c581c93629d35432b2b9463c5` |

`cargo build --release --locked` 通过；安装器通过 Python AST 语法解析。最终 ELF 最大版本符号为 `GLIBC_2.43`，包声明 `libc6 >= 2.43`。本地制品仅供当前 amd64 / glibc 2.43 VM 使用。构建缓存中的 Rust 源与工作区一致；此前 WPS 脚本未再修改。

构建记录：WSL `/var/tmp/sidera-install-d8d04c7-68421ff3/native-startup7-build.log` 和 `native-startup7-package.log`。VM 安装记录：`/var/tmp/sidera-native7-install.log`、`sidera-native7-user-install.log`；用户重登后的只读确认记录：`/var/tmp/sidera-native7-relogin.log`。

回退文件：`/var/backups/sidera-compat-20261024/` 中的 `.6` DEB 和 `sidera-before-native7`；旧用户扩展备份为 `/home/shi01/.local/share/sidera/extension-backups/20261024-153620-26d5d2ea`。本轮没有提交或 push。
