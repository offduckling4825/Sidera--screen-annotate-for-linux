# 原生 Wayland 基础适配与可切换测试环境

> 历史阶段记录：以下版本、安装状态、测试限制与“未提交”等表述仅指当时。当前代码和隔离检查见 [最新复查](REVIEW_2026-10-24.md)，当前使用规则见 [教室指南](../../README.classroom.md)。

本文件记录 `.3` 的历史验证。后续 `.5` 增加 GNOME 原生扩展、多屏、触摸取消、笔迹保留和截图/剪贴板，见 [本次实施与安装记录](GNOME_NATIVE_2026-10-24.md)；新增路径尚待用户交互验收。

日期：2026-10-24；分支：`custom`，在此前 GNOME/XWayland 修复工作区上继续，未提交或推送。

## 安装结果

VMware Ubuntu 26.04.1 amd64 上已覆盖安装 `3.0-Electro-testing+custom20261024.3`，状态 `install ok installed`。正常桌面仍为 GNOME Wayland；新加 Sway 测试窗口，支持在窗口内切换 Sidera 原生 Wayland / XWayland，不要求注销或重启。

| 制品 | SHA256 |
| --- | --- |
| `sidera_3.0-Electro-testing+custom20261024.3_amd64.deb` | `835e42f50943b8c34ab8fcf9ad862675d3d314e7d8ec27fdc9d00ab7ff66b10f` |
| 安装后的 `/usr/bin/sidera` | `48dba86bcfb58940db8403fcf27caa8944507eba78231726107b794467885738` |

`.2` 回退包保存在 `/var/backups/sidera-compat-20261024/`。本包按实际构建 ABI 声明 glibc >= 2.43；不是 Ubuntu 24.04 或麒麟 ARM64 的通用发布包。当前桌面原先运行的 Sidera 不强制退出，下次重开才加载新版。

新增环境包为 Sway `1.11-3`、wlroots `0.19.2-1`、swaybg `1.2.1-1build1`、grim `1.4.0+ds-2build3`、wayland-utils `1.3.0-1`；另安装 foot 和 wlrctl 供诊断。沿用已经修复的 Xwayland `2:24.1.10-1+sidera1`。用户此时已安装 WPS amd64 `12.1.2.29183.AK.preread.sw`，本轮未修改其安装或个人配置。

## 代码修改

- S03：两块 wl_shm 帧各自拥有存储，只能写入已 release 的槽。两个都 busy 时保留最新后备画布和脏区；释放后复制完整最新帧，避免旧槽漏掉中间增量。尺寸变化不改写、不清空 busy 存储，释放后才替换该槽，最多两个在途槽。
- 几何与输入区域：匹配尺寸的缓冲、viewport / buffer scale 和输入区域按事件批次提交，避免重配置时先提交空白或旧尺寸画面。
- 缩放：有 viewporter 时使用分数缩放；缺少所需协议时按整数 scale / surface 协议版本处理，使应用画布与显示后端使用相同有效比例。实际 Sway 测试使用 viewporter；无 viewporter / 老 surface 分支另有单元覆盖，尚未用独立旧版桌面验证。
- 光标：使用 SCTK 带主题的 pointer，在进入覆盖层时设置可见光标；其资源释放遵循协议版本。

未改动 C++ Wayland 后端。没有将多屏、seat 热插拔或触摸手势等未解决问题标为完成。

## 验证

| 验证 | 结果 |
| --- | --- |
| Rust 单元测试 | 28 通过；13 个需图形环境的测试在普通单元运行中跳过，并分别单独执行 |
| 启动诊断 / 后端选择 | 16 通过 |
| Xvfb X11 协议 | 11 通过 |
| wl_shm 存储 / 延迟 release 模拟 | 2 通过：busy 内容不变、积压后完整帧、resize 保留旧存储、旧身份不能释放新缓冲 |
| 安装后程序的原生 Sway 回归 | 关闭 XWayland，6 轮画线→光标→原生 GTK 点击全部通过；最终 7 次按下 / 7 次松开 |
| 安装后程序的 GNOME/XWayland 回归 | 两轮通过；笔迹 776 / 1552 像素逐值保留，侧栏拖动后点击 4/4，显示层无重映射 |
| 可见嵌套测试桌面 | 原生模式画线后计数 2/2；切到 XWayland 对照后 3/3；200% 显示与回到 100% 已检查 |
| 开关流程 | 多次打开/关闭通过；最后实际点击“结束测试，返回 Ubuntu”，测试进程退出、控制文件移除，GNOME 会话进程未变化 |

原生回归从 **1920×1080 首次 configure** 开始，覆盖原 S01 占位画布问题的实际协议路径。后续 1280×800、1600×1000 / 125%、1920×1200 / 150%、1920×1200 / 200%、回到 1280×800 均通过。各轮可见红色笔迹像素为 466、466、708、1013、1748、466；切光标前后像素逐值一致，绘图时原生下层窗口未收到点击，恢复光标后收到完整按下与松开。

这些比较发生在同一轮绘图与光标切换之间；不表示缩放前后的旧笔迹被保留。现有设计在 resize / scale 时重建画布，测试面板已提示，旧页缓存问题 S21 仍开放。

虚拟机证据目录：

- `/var/tmp/sidera-native-installed-result/`：最终安装文件的 `result.json`、buffer 测试输出、Wayland 日志和逐轮截图。
- `/var/tmp/sidera-native-mutter-regression/`：最终安装文件的 GNOME 隔离回归。
- `~/.local/state/sidera-wayland-lab/`：可见测试窗口的独立环境、日志。

可复用的测试入口与安装方法见 [Wayland 测试桌面说明](../../compat/wayland-lab/README.md)。测试中使用私有进程、网络、家目录及 D-Bus，不向当前桌面注入测试点击。

## 支持边界与后续

目前完成的是支持 layer-shell 的桌面上的单输出原生绘图、鼠标穿透及缩放基础验证。GNOME 主桌面仍走此前已验证的 XWayland 路径；安装 Sway 并未改变 GNOME 提供的协议。

多屏 / 输出与 seat 热插拔、物理触屏、截图和剪贴板、全局热键、WPS 完整联动，以及其他发行版、架构和最低 ABI 仍待验证或修复。后续按 S16–S21 分项继续，不将本轮结果等同于全部 Wayland 功能适配完成。
