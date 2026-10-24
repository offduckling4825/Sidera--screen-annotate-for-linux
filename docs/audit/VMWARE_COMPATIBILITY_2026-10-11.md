# VMware 启动兼容性记录

> 历史阶段记录：以下版本、安装状态、测试限制与“未提交”等表述仅指当时。当前代码和隔离检查见 [最新复查](REVIEW_2026-10-24.md)，当前使用规则见 [教室指南](../../README.classroom.md)。

日期：2026-10-11；分支：`custom`；修改前基线：`96c0262`。本次使用真实 VMware 来宾系统，不把 WSLg 的结果等同于 VMware、Ubuntu 24.04 或麒麟验收。

## 环境与已确认事实

| 项目 | 实测 |
| --- | --- |
| VMware 名称 | Ubuntu 64-bit |
| 系统 | Ubuntu 26.04.1 LTS，amd64，内核 7.0.0-30-generic |
| 资源 | 4 vCPU、6 GiB 内存；桌面 1280×800、缩放 1 |
| 桌面 | GNOME Shell 50.1-0ubuntu1.2，Wayland 会话 |
| 图形组件 | Xwayland 2:24.1.10-1，Mesa 26.0.8-1ubuntu0.3，vmwgfx |
| glibc | 2.43 |
| 已安装包 | sidera 3.0-Electro-testing，amd64，dpkg 状态 `ii` |
| 已安装程序 SHA256 | `127f543062915a0cf051f45312f24789c8099c69b7dbb8dee1a1d7e5acdd52b9` |

dpkg 历史显示软件包已在 2026-10-02 配置完成。本次复现的是**启动失败**，没有复现 DEB 安装失败。用户最初描述为 Ubuntu 24.04，随后确认排查当前这台 VMware；来宾 `/etc/os-release` 与 VMware Tools 均报告 26.04.1。

## 两条启动路径

1. 上游已安装程序在当前 Wayland 会话下输出 `the requested global was not found in the registry`，然后以 **0** 退出。
2. 以临时环境变量选择 X11 后，程序连接到 Xwayland 并报告 1280×800。随后 Xwayland 发生段错误，GNOME 记录 `Connection to xwayland lost`、`GNOME Shell crashed with signal 11`，用户桌面退出至登录页。该路径**失败**，不是“兼容模式已可用”。用户重新登录后继续进行只验证 Wayland 失败诊断的测试。

两次启动均使用临时家目录与 bubblewrap，WPS 桥关闭，没有覆盖已安装的 Sidera，也没有修改用户 WPS 登记或桌面配置。测试隔离用户文件不等于隔离实际显示服务器，因此第二项仍影响了桌面会话；后续可能触发显示服务器错误的探针应放在独立嵌套桌面或可还原测试 VM 中。

Xwayland 24.1.11 的上游修复记录包含 `damage_report()` 空指针问题，见 [Debian 上游代码同步记录](https://lists.debian.org/debian-x/2026/04/msg00132.html) 与 [上游修复提交](https://gitlab.freedesktop.org/xorg/xserver/-/commit/36f53145e415bbc959cdf2ed8bb0cb4f7c4c1f13)。这是排查方向：本机尚未取得符号化崩溃栈，也未验证升级后是否消失，不能断言就是同一个缺陷。当前虚拟机已有 apt 索引的候选版本仍为 24.1.10，本次没有改用其他发行版软件源或替换系统图形组件。

## 已实施的应用修复

- 明确的 `XDG_SESSION_TYPE=x11` 优先于继承的 Wayland 变量；空的 `WAYLAND_DISPLAY` 不代表 Wayland 会话。未标明会话时仍识别非空 `WAYLAND_DISPLAY`/`WAYLAND_SOCKET`。
- 增加 Rust 专用 `SIDERA_BACKEND=auto|x11|wayland`，非法值直接报告；默认 `auto`。显式选择只控制入口，不改变系统会话，也不证明该后端能完整运行。
- Wayland 错误包含无法连接、无法读取协议或缺少 `wl_compositor` / `zwlr_layer_shell_v1` / `wl_shm` 的具体上下文。保留一次连接、一次绑定的原有流程，不做额外重复探测。
- 启动/运行错误由入口统一返回退出码 1；X11 调试冒烟的正常退出以及唤醒已有实例仍为 0。单实例绑定失败不再与“已有实例”混为一谈；并发启动竞态仍属于 S24。
- 最近一次错误记录在 XDG state 目录的 `sidera/startup-error.log`，默认 `~/.local/state/sidera/startup-error.log`。可选 `notify-send` 通知最多等待两秒；缺少通知服务或日志无法写入不改变原始失败状态。
- 不自动回退 XWayland；本轮未实现 GNOME 原生覆盖层、未修改 C++ 启动路径。

## 验证与证据边界

- WSL Ubuntu 26.04 构建副本使用 rustc/cargo 1.93.1，`cargo build --locked --offline` 通过。24 项 Rust 单元测试在临时家目录与隔离网络中通过，包含 7 项新的后端选择测试。
- 2026-10-16 收尾尝试运行 `cargo clippy --all-targets --locked --offline`，本地工具链没有安装 Clippy（`no such command: clippy`），该项未执行；不记为检查通过。
- `python3 rust/tests/test_startup.py /absolute/path/to/sidera` 的 12 项回归全部通过，使用真实程序、独立 Xvfb 与临时家目录，覆盖会话选择、失败状态/日志、保留已有实例唤醒和通知退出行为。依赖 Python 3、bubblewrap、Xvfb；缺依赖直接失败，已接入现有 Rust CI job。工作流触发分支未变，未声称远程 CI 已运行。
- WSLg 的 `/tmp/.X11-unix` 可能无法创建测试 socket；在 WSL 运行上述测试时，外层也使用 bubblewrap 的独立 `/tmp` 与网络。普通 Linux CI 的 Xvfb 使用自己的动态 display 编号。
- 修改后的临时程序在 VMware 当前真实 Wayland 会话中，明确报告缺少 `zwlr_layer_shell_v1`，退出码为 1，诊断文件内容一致。没有启动 Xwayland，原桌面会话继续存在。临时程序用于验证代码，未覆盖 `/usr/bin/sidera`。
- 首次通知实现未在 VMware 画面中观测到提示，随后补上发送等待及超时限制；修订后的通知行为通过上述自动化测试，真实桌面通知显示仍待验收。2026-10-16 按用户要求停止继续排查并收尾提交。
- 本次构建来自 glibc 2.43 环境，仅验证当前 VM 的诊断行为；不是 Ubuntu 24.04 或麒麟发布包。尚未验证支持 layer-shell 的合成器、真实触屏、课件联动或完整 GNOME 批注功能。

原始运行输出暂存于宿主的 `C:\Users\shi01\AppData\Local\Temp\sidera-vmware-20261011`：`diagnose.txt`、`old-startup.txt`、`desktop-errors.txt`、`crash-details.txt`、`new-startup.txt`。其中没有保存登录密码；崩溃转储本体保留在来宾系统，没有纳入仓库。

## 下一阶段

1. 在独立测试显示服务器里取得 Xwayland 符号化栈，核对是否命中上述上游修复；优先验证该发行版提供的修复版，不混装其他发行版图形库。
2. 系统崩溃排除后，单独验收 XWayland 的覆盖层、置顶/穿透、原生 Wayland 窗口上的操作、翻页与全局热键。只有实际通过的功能才列为支持。
3. 若要完整支持 GNOME Wayland，需要设计其允许的覆盖层/桌面集成方案；设置一个环境变量不能替代此项开发。
4. Ubuntu 24.04 建立独立 VM 验证；麒麟机器先采集现有 U 盘诊断脚本的报告，再准备与其 glibc 和 arm64 匹配的包。
