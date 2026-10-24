# GNOME 原生扩展与 Wayland 功能补齐

日期：2026-10-24。分支 `custom`，基于已有未提交兼容性/WPS 修改继续；本轮未提交、未推送。用户明确选择增加 GNOME 原生扩展，并要求由其自行进行桌面测试。

后续只读诊断：用户注销重登后，扩展已发现但为 `ERROR`，错误是 GNOME 50 已移除 `Meta.is_wayland_compositor()`。扩展版本 2 已加入兼容判断；后续 `.6` 改动与部署状态见 [记录](TOUCH_UI_2026-10-24.md)。下面的“扩展尚未发现”描述 `.5` 初次安装时状态，不代表本次重登后的结果。

后续确认：用户安装 `.6` 后再次注销登录，扩展版本 2 状态为 `ACTIVE`，本地桥 socket 已建立。但用户实际启动标准入口失败；扩展创建覆盖层时异常，未发送初始化回应。版本 3 修正 GNOME 50 已移除的覆盖层参数，并补充错误回传；见 [最新启动修复记录](GNOME_STARTUP_2026-10-24.md)。扩展加载成功不能作为 Sidera 原生启动成功的证明。

## 已安装的制品

VMware Ubuntu 26.04.1 amd64 / GNOME 50.1 Wayland 已覆盖安装 `3.0-Electro-testing+custom20261024.5`，dpkg 状态 `install ok installed`。

| 制品 | SHA256 |
| --- | --- |
| `sidera_3.0-Electro-testing+custom20261024.5_amd64.deb` | `3f631fce94c305d1daa118cfc486a8dc41739a62d8fd6896549c2ac00130983e` |
| `/usr/bin/sidera` | `e700331ddb91ad74c12382e07320403f187222bba72ccd0ccb8482120fb67ffa` |
| 用户扩展 `extension.js` | `06e8bc711973eb2ba4a622a4992d288a483eb8580a7ed99e28875aa068c01417` |

本地构建的最大 GLIBC 符号为 `GLIBC_2.43`，control 相应声明 `libc6 >= 2.43`；直接动态依赖为 libgcc_s、libm、libc。**此 amd64 包仅用于当前 VM，不是 Ubuntu 24.04 或麒麟 ARM64 的通用包。** 仓库发布脚本仍须通过最低 ABI 发布门禁，不用本次制品冒充旧系统构建。

安装器已将扩展复制到 `/home/shi01/.local/share/gnome-shell/extensions/sidera-native@sidera.local` 并加入 enabled-extensions，保留已有桌面扩展。当前 Shell 查询仍提示新扩展不存在，首次发现需注销、重新登录；这不等于扩展已经运行通过。已向用户说明并请求其保存内容后自行重新登录。安装没有停止现有 Sidera、WPS 或桌面。

回退文件：`/var/backups/sidera-compat-20261024/sidera_3.0-Electro-testing+custom20261024.4_amd64.deb` 和 `sidera-before-native5`。原 Xwayland 修复包保持不变。GNOME 原生与兼容模式入口均已安装，独立 Sway 测试环境也更新了缩放说明和 `Ctrl+Shift+D` 绑定。

## 实现范围

- `gnome.rs` 与 GNOME 扩展：同用户 Unix socket、有界双向消息、版本/输出代次检查、脏块渲染、独立透明输入 actor、全局切换和紧急释放、锁屏/概览暂停、原生截图与剪贴板。按下期间保留输入 actor，切模式不重映射显示 actor；断线/失联移除全部覆盖层。
- `desktop.rs`：GNOME 和 layer-shell 共用逐屏画布、工具同步、白板页同步、单个 WPS 事件源、断开屏幕暂存与重新连接。
- `wayland_session.rs`：每输出创建 layer，每 seat 管理输入/虚拟设备，输出移除不退出进程；按 wl_touch.frame 聚合接触尺寸/数量，取消清理输入状态；v2 虚拟指针映射目标输出，右键等物理释放和输入区提交后转发。
- `app.rs`：原子几何更新，按相对位置保留当前笔迹，旧页缓存恢复时适配尺寸；重建后重置撤销。相同配置不损失撤销或侧栏位置。
- `x11.rs`：根几何和资源变化同步覆盖层，保留笔迹；XWayland 截图经 Portal，停止可能触发 RemoteDesktop 授权的 XTEST 右键转发。
- `portal.rs`、`clipboard.rs`：提前订阅截图响应、并发/超时/关闭请求、URI 解码，持久剪贴板拥有者与 Wayland data-control。
- WPS 基本行为沿用用户确认成功的 `.4`，未就绪只变按钮颜色；新增多屏只共享一个联动桥。

未重构 C++ 后端。GNOME 不提供桥所需的触摸直径，不能保证物理面积掌擦。layer-shell 的键盘、虚拟指针、Portal、剪贴板和全局快捷键取决于桌面能力；没有协议时不自动提权。详细协议、使用方法与人工检查清单见 [扩展说明](../../compat/gnome-extension/README.md)。

## 本轮实际检查

| 项目 | 结果 |
| --- | --- |
| `cargo check --tests --locked` | 通过，仅编译测试代码，未执行 |
| `cargo build --release --locked` | 通过 |
| GNOME JavaScript 语法 | Node `--check` 通过，未执行 GNOME 模块 |
| 安装器 Python 语法 | AST 解析通过 |
| GSettings schema | `glib-compile-schemas --strict` 通过 |
| DEB / ELF / 安装状态 | 包资源、架构依赖、版本与程序校验和已核对 |
| 图形操作、触摸、多屏、截图粘贴、WPS 新版回归 | 未执行，由用户测试 |
| GNOME 46–49、KDE、麒麟 ARM64 | 未执行；接口兼容实现不代表这些系统实测通过 |

本轮没有启动应用进行自动操作，没有注入鼠标/键盘，没有运行嵌套桌面回归，也没有替用户注销/重启。此前 `.2`–`.4` 的通过结果保留为历史证据，不能作为本次新增 GNOME 原生路径的验收结果。

构建记录位于 WSL `/var/tmp/sidera-install-d8d04c7-68421ff3/gnome-native-build-final.log` 和 `gnome-native-package.log`；安装记录在 VM `/var/tmp/sidera-native5-install.log` 与 `/var/tmp/sidera-gnome-user-install.log`。
