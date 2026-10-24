# 兼容性修复与验收记录

> 历史阶段记录：以下版本、安装状态、测试限制与“未提交”等表述仅指当时。当前代码和隔离检查见 [最新复查](REVIEW_2026-10-24.md)，当前使用规则见 [教室指南](../../README.classroom.md)。

日期：2026-10-24；分支：`custom`。本轮继续处理 [2026-10-11 VMware 启动问题](VMWARE_COMPATIBILITY_2026-10-11.md)，并结合学校电脑的诊断报告划定目标环境。

后续已安装 `.3` 并加入可开关的 Sway 原生 Wayland 测试窗口，最新版本、哈希与验收见 [原生 Wayland 记录](NATIVE_WAYLAND_2026-10-24.md)。下文保留此前 `.2` XWayland 阶段的结果与用户验收，不作为当前安装版本清单。

**当前结论：当前 VM 的 Xwayland 崩溃已修复；`.2` 已改用显示/输入分层方案，正式隔离回归确认穿透、笔迹保留及显示层持续可见，并已覆盖安装。用户在当前真实 VM 复测后明确确认鼠标/画笔切换不闪、穿透正常；物理触屏、WPS、其他发行版等完整兼容性尚未验收。**

## 1. 实际环境

| 项目 | VMware 图形虚拟机 | 学校电脑诊断报告 |
| --- | --- | --- |
| 系统 | Ubuntu 26.04.1 LTS，amd64 | 银河麒麟桌面 V10 SP1，aarch64 |
| 内核 | `7.0.0-34-generic` | `5.4.18-53sy01-generic` |
| 处理器 | VMware x86_64 | Phytium D2000/8 E8C |
| 桌面会话 | GNOME Shell `50.1-0ubuntu1.2`，Wayland | UKUI，原生 X11 |
| 显示环境 | `DISPLAY=:0`、`WAYLAND_DISPLAY=wayland-0`；1280×800，缩放 1 | `DISPLAY=:0`、`WAYLAND_DISPLAY` 未设置；会话报告 `x11` |
| 基础运行库 | glibc 2.43；Mesa `26.0.8-1ubuntu0.3` | glibc 2.31；Qt Core/Gui/Network 5.12.8、Widgets 5.12.12 |
| Sidera | 已覆盖安装 `3.0-Electro-testing+custom20261024.2` amd64 | 报告中已装 `2.6-Geo-stable` arm64 |
| Xwayland | 原 `2:24.1.10-1`，现为本地回补 `2:24.1.10-1+sidera1` | 当前使用原生 X11；本轮没有替换其显示组件 |
| WPS | 本轮未验收 | 已装 `12.1.2.28080.AK.preread.sw` arm64；未验收 Sidera 联动 |

学校报告来源为 `E:\sidera_check\logs\sidera-check-20261011-170435-1bW5id.txt`。报告中的 X11 Composite、RANDR、SHAPE、XInputExtension、XTEST 查询成功，但采集脚本未启动应用做功能测试。图形窗口和 WPS 能运行，只能说明存在图形环境，不能据此判断使用 Wayland 或提供 layer-shell。

当前 VM 实际是 Ubuntu 26.04.1，不是用户最初描述的 Ubuntu 24.04。**Ubuntu 24.04 尚未实测。** 本轮没有构建或安装面向学校 ARM64/glibc 2.31 的新包；Ubuntu 26.04 的 amd64 制品不能用于该机。

最终 Sidera 包的安装状态为 `install ok installed`，安装后文件哈希与本轮制品一致：

| 最终 Sidera 产物 | SHA256 |
| --- | --- |
| `sidera_3.0-Electro-testing+custom20261024.2_amd64.deb` | `2101c362f10e47fd08c46469b34553ac82b4da277cff23595f68d47918cf631c` |
| `/usr/bin/sidera` | `a2177eaefeab0f0992d2c1514dedcf9f6fab8dbdd40b330105f73f1b3628d867` |

前一版 `.1` 回退包保存在来宾系统 `/var/backups/sidera-compat-20261024`。其 DEB SHA256 为 `68aa4c9fc7e2131f76a0a71ba2af514e8253a5bd7b0a63144c1e5e6bc24cc886`；该版用户已确认穿透恢复，但有鼠标/画笔切换闪烁。

## 2. 启动路径修改

GNOME Wayland 不提供 Sidera 原生 Wayland 覆盖层要求的 `zwlr_layer_shell_v1`。本轮保留明确的会话判断和 `SIDERA_BACKEND=auto|x11|wayland` 选择，在 Rust 入口增加有条件的回退：

- `auto` 且 Wayland 连接成功、协议列表中缺少 layer-shell、`DISPLAY` 非空：尝试 X11/XWayland，并在日志说明全局热键、原生 Wayland 窗口的翻页/截图等能力限制。
- 显式 `wayland`、无法连接 Wayland、读取协议失败或其他协议错误：返回具体错误，不将故障当成回退条件。
- 原生 Wayland 能完成能力检查时继续使用同一次连接及已准备的对象，避免检查与运行连接不一致。

X11 入口在创建窗口前检查**实际连接服务器**的 XWAYLAND 扩展、setup release number 和 vendor。对 release `12401010`（24.1.10），仅本轮确切修复标记 `The X.Org Foundation (Sidera damage-report fix)` 放行；其他 vendor 保守拒绝并提示修复显示服务器。原生 X11 不受这条 Xwayland 特定保护影响。

该保护不根据本机 `dpkg` 猜测远程服务器版本，不自动安装系统更新，也不是其他服务器版本的兼容性认证。其他发行方即使回补同一修复、仍沿用普通 vendor，也可能被保守拦截，需另行确认来源和验证。

启动错误仍以非零状态退出，并记录至 `${XDG_STATE_HOME:-$HOME/.local/state}/sidera/startup-error.log`。这些变更仅适用于当前 `custom` Rust 版，不改变旧发布包或 C++ 入口。

## 3. Xwayland 崩溃定位与本地回补

原 Ubuntu 官方 Xwayland 24.1.10 二进制和调试符号具有相同 Build ID：`0d748fbe3aab20c19c4af7bf3fe577972db83324`。旧崩溃的关键偏移映射为：

```text
0x5be4a   damage_report (hw/xwayland/xwayland-window.c:321)
0x11dd05  damageRegionAppend
0x1218e5  damagePolyFillRect
0xa8797   miPaintWindow
0xa6f98   miWindowExposures
0xb415f   miHandleValidateExposures
0x100c9c  MapWindow
0xc1597   ProcMapWindow
0xca994   Dispatch
```

故障指令读取空对象的 `0x40` 偏移，与原日志的 `Segmentation fault at address 0x40` 一致。窗口映射的初始填充可以在对应 Wayland 窗口尚未建立时触发损伤报告；单纯等待选择区所有者或移除初始背景填充不能证明后续绘制安全，因此没有将这类时序规避当成系统修复。

采用上游提交 [36f53145e415bbc959cdf2ed8bb0cb4f7c4c1f13](https://gitlab.freedesktop.org/xorg/xserver/-/commit/36f53145e415bbc959cdf2ed8bb0cb4f7c4c1f13)：在 `damage_report` 中检查空对象，并在建立 surface 失败时取消对应 damage 注册。源码改动为 12 行 diff；没有混装其他发行版图形库。

本地构建以 Ubuntu 同版 `24.1.10-1` 官方源包为基础，保留 Debian 构建规则与 hardening，追加该补丁及 vendor 标记。源包来自 `security.ubuntu.com/ubuntu/pool/main/x/xwayland/`；归档 SHA256 与 `.dsc` 一致。`.dsc` 的签名因缺少 Debian 上传者密钥未验证，不能把哈希核对表述为签名验证。

| 产物信息 | 值 |
| --- | --- |
| 本地包版本 | `2:24.1.10-1+sidera1` |
| 服务器 vendor | `The X.Org Foundation (Sidera damage-report fix)` |
| setup release number | `12401010` |
| 修复 DEB SHA256 | `8a03a9028e0de9d06718c141219c4cd4ff1fedeaca3a71ea72e28b767d7b21ff` |
| 修复 `/usr/bin/Xwayland` SHA256 | `295124beb1e08335463d961ae679fdd4f755a26bab53330a2b87e5b95796e9d4` |
| 原官方回滚 DEB SHA256 | `f620802e59cb10a4d66e0ed2abf16ad425be1ab2b7a3deef1c2b367647c2c7a5` |

本地包已在当前 VM 安装，并在重启图形会话后从正在运行的 X 服务器握手确认 vendor 与 release number，确认使用的不是升级前残留进程。**这是当前 VM 的本地回补包，不是 Ubuntu 官方更新，也不是通用兼容包。** 它是 amd64，并依赖 `libc6 >= 2.43`、`libgcrypt20 >= 1.12.0`，不能用于 Ubuntu 24.04 或 ARM64 麒麟。

补丁、源码来源校验及构建规则保存在 [compat/xwayland-24.1.10](../../compat/xwayland-24.1.10/README.md)。该目录不包含系统自动安装步骤或二进制；修复包与原官方回滚包留在宿主交付目录。构建期间两个上游 XTEST 包装测试因缺少 `XTEST_DIR` 跳过，不计作通过。

## 4. 已完成验证

| 检查 | 结果与范围 |
| --- | --- |
| Rust 单元测试 | 26 项通过；包括条件回退和服务器保护规则 |
| 隔离启动回归 | 16 项通过；使用真实程序、临时目录与隔离显示，验证后端选择、失败诊断和有条件回退 |
| X11 协议回归 | 最终 `.2` Xvfb 下 11 项通过，覆盖输入区域、分层、重映射及 XInput 触摸订阅 |
| 原版崩溃复现 | 独立 headless Weston + 手动 rootless Xwayland、无窗口管理器；原 Sidera 创建窗口时服务器确定性段错误，最终退出 `-6`（SIGABRT），栈与旧崩溃一致 |
| 同条件回补对照 | 同一探针连接修复服务器，应用连续运行 3 秒，服务器存活；未再触发该空指针崩溃 |
| GNOME 隔离冒烟 | headless GNOME + 修复服务器，Sidera 自动回退 XWayland 后运行 8 秒 |
| 最终 `.2` 包的隔离点击回归 | Clutter 虚拟鼠标经过合成器真实投递，两轮画笔/光标切换后，原生 Wayland GTK4 接收端都收到完整 press/release；原无刷新对照首轮穿透失败 |
| 最终 `.2` 笔迹保留 | 两轮分别有 776 / 1552 个非零笔迹像素，各轮切换前后逐值完全一致 |
| 最终 `.2` 拖动与显示稳定性 | 侧栏下移 45 px 后选笔、切光标和穿透通过；显示窗口无 Map/Unmap/Destroy，合成器截图侧栏与红线持续显示 |
| 早期 `.1` 阶段探针 | 无后续鼠标输入等待 1.2 秒后整层恢复，918 个非零笔迹像素逐值保留；另闲置 2.4 秒，0 次额外重映射。这不是 `.2` 的最终显示稳定性结果 |
| 真实图形会话 | Xwayland 修复安装、重启并核对 vendor；`.1` 用户确认穿透恢复但报告切换闪烁。`.2` 覆盖安装并重开后，用户明确确认鼠标/画笔切换不闪、穿透正常 |

Rust X11 触摸订阅修复先协商 XInput 2.2，再在覆盖层选择 TOUCH_BEGIN/UPDATE/END/OWNERSHIP，删除只能选在根窗口的 RAW_TOUCH 事件，并检查服务器返回状态。协议测试验证的是订阅行为，不是物理触屏、手掌橡皮或点击穿透。

启动套件入口为 `python3 rust/tests/test_startup.py /absolute/path/to/sidera`；X11 协议套件在 `rust/` 中用 `xvfb-run -a cargo test --locked x11:: -- --ignored --test-threads=1` 显式执行。单元和启动验证使用隔离家目录与网络，避免影响用户实际 WPS 登记及已有桥服务。此处记录本地测试结果，不代表远程 CI 已运行。

最终包已通过正式命令 `python3 rust/tests/test_mutter_passthrough.py /absolute/path/to/sidera --check-stable-visual`，可用 `--output-dir` 指定一个新的日志目录。它要求 Linux、bubblewrap、dbus-run-session、支持 headless 的 GNOME Shell、Xwayland、Python GObject introspection、GTK4 introspection、libX11 与 libXcomposite；通过独立 PID/网络、家目录、显示与会话总线启动 headless GNOME，不连接当前桌面。它验证两轮 GTK4 实际收到 press/release，并在 `--check-stable-visual` 下检查笔迹逐值保留、侧栏拖动以及显示窗口没有重映射或销毁，不使用仅作用于 X11 的 XTest 注入。该手工集成测试不属于普通 CI。早期 `.1` 的闲置重映射计数来自独立探针，不与最终包结果混用。

主要原始日志位于宿主 `C:\Users\shi01\AppData\Local\Temp\sidera-vmware-20261024`：`current.txt`、`damage-old.txt`、`damage-fixed.txt`、`guard.txt`、`gnome-fixed.txt`、`install.txt`、`desktop-test.txt`；最终 `.2` 安装记录为 `split-input-install.log`，早期 `.1` 安装及探针分别为 `input-refresh-install.log`、`shape-race-review-result-final-package.txt`。正式隔离脚本将 `result.json`、应用/接收端/合成器日志写入其打印的测试目录。本地 Xwayland 构建目录为 WSL `/var/tmp/sidera-xwayland-backport-20261024`；宿主交付目录为上述临时目录的 `xwayland-backport`。临时日志和安装包未纳入仓库，不包含在仓库材料的长期可用性保证中。

## 5. 光标模式点击穿透：定位与修复

覆盖层能够显示后，用户报告从绘图切回普通光标模式仍无法点击底层窗口。这与上节 Xwayland `damage_report` 崩溃是两个独立问题；显示服务器不再崩溃，并不意味着输入会正确送给底层应用。

### 观察与根因证据

故障状态下，XShape 查询已反映光标模式预期的两侧输入区域；`XQueryPointer` 的按钮掩码为 0，未发现鼠标按钮持续按下。原生 Wayland GTK4 接收窗口在 `draw → cursor` 后收不到点击，但此时新执行的 Clutter actor pick 能命中该 GTK 窗口，说明当前几何命中结果与实际输入投递目标不一致。

检查 Mutter 50.1 的 [Clutter stage 实现](https://raw.githubusercontent.com/GNOME/mutter/50.1/clutter/clutter/clutter-stage.c)可见，指针仍落在缓存的 `clear_area` 时，输入更新路径可以沿用旧目标；[surface 输入区域更新](https://raw.githubusercontent.com/GNOME/mutter/50.1/src/compositor/meta-surface-actor.c)仅替换 `input_region`，没有在该函数中触发设备目标缓存失效。结合下述对照，本机问题定位为输入区域变更后仍使用旧 actor 的缓存状态；这不是已得到上游确认的通用缺陷结论。

在独立 headless GNOME 中，使用 Clutter 虚拟鼠标经过合成器的真实输入投递路径，以原生 Wayland GTK4 窗口记录接收到的点击，完成以下对照：

| 操作或对照 | 观察 |
| --- | --- |
| 画笔模式切回光标模式 | 可复现底层 GTK4 窗口收不到点击 |
| 去掉触摸订阅的候选版本 | 仍失败，不能把这次穿透问题归因于触摸订阅 |
| 普通 hover 或 redraw | 不能恢复投递 |
| 在合成器探针中仅触发 `queue_relayout` | 点击恢复，支持输入目标缓存失效路径的判断 |
| 对同一 X window 执行 unmap/map | 点击恢复；不再处理下一次输入区域变化时，后续模式切换仍会再次失败 |

`queue_relayout` 是隔离定位探针，不是应用向用户桌面注入的常规修复。验证依据是 GTK4 接收端实际得到点击；仅看到 XShape 正确、光标移动或新的 actor pick 正确，都不足以判定穿透通过。

### 应用侧修复与验收边界

修复仅对实际连接为 Xwayland、且 EWMH 窗口管理器名称确认为 `GNOME Shell` 或 `Mutter` 的路径启用。早期 `.1` 将同一覆盖层安全重映射，用户确认穿透恢复，但其短暂隐藏显示层造成鼠标/画笔切换闪烁。最终 `.2` 将显示与输入分离：

- 显示窗口在首次映射前就把输入区域设为空，之后保持映射；`present` 只写显示窗口，侧栏与笔迹留在该窗口。
- 另建透明 ARGB32 输入窗口承接 XShape 与输入事件。显示、输入两窗口均必须具备 32 位透明视觉，条件不满足时不继续该分层路径。
- 输入区域内容先去重，只有真实变化才安排刷新；等待鼠标按钮全部松开，且触摸、拖动、滑块和绘制均结束后，只对**同一输入窗口 XID** 做 unmap/map，显示窗口不参与重映射。
- 单实例激活先恢复显示窗口置顶，再恢复输入窗口置顶，保证显示和事件接收的相对顺序。活动按压期间不重映射，以免取消 grab 或丢失 release。

触摸刷新门禁独立记录每个 XInput TouchBegin/TouchEnd，不依赖原手势状态的 1.5 秒超时清理。触点静止不等于已释放；若丢失 End，刷新会保守延后。这避免在实际按压仍持续时重映射，但不解决原有触摸取消/丢 End 生命周期问题，也不构成物理触屏验收。

最终 `3.0-Electro-testing+custom20261024.2` 制品已经通过第 4 节的 26 项单元、16 项启动、11 项 X11 协议回归，以及带 `--check-stable-visual` 的正式隔离测试。两轮原生 Wayland GTK4 完整点击均通过，776 / 1552 个非零笔迹像素各轮切换前后逐值一致，侧栏拖动 45 px 后仍能选笔、切光标并穿透；显示窗口没有 Map/Unmap/Destroy，合成器截图侧栏与红线持续显示。原无刷新对照在首轮穿透失败，证明测试能够暴露原故障。

`.1` 的闪烁已针对性改为上述分层方案，`.2` 隔离显示持续性已验证且已覆盖安装。用户关闭旧实例后重开 `.2`，明确回复“不闪了，穿透也正常”，确认当前 VM 的鼠标/画笔切换和点击穿透已恢复。该反馈不扩大为完整课堂验收；白板/弹窗、滑块、物理触摸、其他目标应用和完整课件操作仍按剩余验收跟踪。

## 6. 尚未通过的验收与下一步

1. **扩大已通过交互的验收范围。** 当前 VM 的鼠标/画笔切换不闪与穿透正常已获用户确认；继续覆盖更多原生 Wayland 与 XWayland 目标窗口、侧栏/弹窗边界、持续绘制和退出恢复，不将这两项交互的确认等同于完整授课验收。
2. 在当前 VM 验收实际绘图、撤回、白板、截图/粘贴、置顶、全局热键、翻页及会话恢复；启动 3 秒或 8 秒没有覆盖这些功能。
3. 使用真实 WPS 演示验证加载项、页内动画、翻页、批注分页缓存和退出恢复；本轮未完成 WPS 联动测试。
4. 建立真正的 Ubuntu 24.04 测试环境，重新确定 ABI、依赖及系统包修复策略；不直接安装本次 26.04 回补包。
5. 为学校麒麟 ARM64/glibc 2.31 单独构建并验证目标包，优先走已确认的原生 X11；之后做真实触屏、手掌识别和课堂 WPS 验收。

S18 的后端能力表达和 S20 的触摸生命周期仍保持未完成。原生 GNOME Wayland 覆盖层支持也没有因 XWayland 回退而实现。
