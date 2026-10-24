# Sidera 教室部署指南

适用于本仓库 `custom` 分支的 Rust 3.0 定制版。Sidera 原项目与本分支归属见 [NOTICE.md](NOTICE.md)，功能概览见 [README](README.md)。

## 安装前

核对发行版、CPU 架构、系统 libc 与桌面会话。本次 `custom20261024.29` DEB 只提供 **amd64 / libc6 ≥ 2.43**，已安装核对的环境是 VMware Ubuntu 26.04.1。它不是 Ubuntu 24.04、ARM64 或银河麒麟通用安装包。

匹配系统请按 [README 的安装步骤](README.md#安装与开始使用) 下载、校验并用 apt 安装；不要强制跳过依赖。Rust 与旧 C++ 版使用同一个 `sidera` 包名，安装会覆盖原版本。

## 桌面配置

X11 可使用普通入口。GNOME Wayland 原生模式需要以登录桌面的普通用户运行：

```sh
sidera-gnome-install
```

安装或更新已加载扩展后，保存工作并注销、重新登录，然后使用“Sidera（GNOME 原生）”。需要兼容路径时选择“Sidera（兼容模式）”。切换前先退出正在运行的 Sidera，细节见 [GNOME 扩展文档](compat/gnome-extension/README.md)。

不要以 root 启动 Sidera，也不要为排查问题直接替换整套系统图形库。

## WPS 联动

安装包不包含 WPS。请使用匹配系统的 WPS Linux 版本；加载项功能取决于 WPS 提供的 API。

1. 关闭 WPS，普通用户执行 `sidera-wps-addin-install`。
2. 启动 Sidera，在设置中开启“WPS 联动”。
3. 打开 WPS 演示，确认加载“批注联动”标签并取得状态。
4. 文档支持“当页开始”时，侧栏出现绿色“开始放映”；放映时显示“结束放映”。未接入时显示 Esc，并按桌面能力使用按键模拟。

加载项安装器只更新 Sidera 条目，修改前备份已有登记。首次登记或更新加载项后重新打开 WPS。不要删除整个 `publish.xml`，其中可能包含其他加载项。详细机制与限制见 [WPS 文档](wps-addin/README.md)。

## 每节课

- 需要书写时选择画笔，绘图工具组自动展开；清除或在空画布切回光标时按规则收回。
- 光标模式下可操作底层课件。整栏收起只留边缘小角，需主动点击展开。
- 联动取得页码后，点页码打开拨轮，选好再点“跳转”；未联动时不显示未知课件页码。
- 白板进入后使用画笔。关闭再开保留**本次运行**的内容；退出程序后内容不会自动保存，请在退出前截图保存需要的内容。
- 掌擦阈值和橡皮倍率可按设备调整，也可逐项恢复默认。GNOME 原生触摸缺少本桥需要的接触直径，面积掌擦不能作为已保证的功能，多指路径仍可用。
- 自动启动是用户设置；安装 DEB 不会替你退出桌面或启动应用。

## 更新

保存文档与需要保留的批注，退出 Sidera 后安装新版。主程序重开才会加载更新；WPS 加载项更新后重新打开 WPS；GNOME 扩展更新后执行安装器并注销重登。仅更新主程序时通常不需要注销桌面。

## 排查与反馈

- 启动失败：`${XDG_STATE_HOME:-$HOME/.local/state}/sidera/startup-error.log`。
- WPS 联动：`~/wps-api-debug.log`。
- GNOME 扩展：`journalctl --user -b -g Sidera`。
- 在设置中查看系统诊断，记录实际后端、包版本、缩放比例与输入设备。

日志可能包含本机路径和文档信息，提交前脱敏。普通问题按 [CONTRIBUTING.md](CONTRIBUTING.md) 反馈，安全问题按 [SECURITY.md](SECURITY.md) 私密报告。

## 卸载

```sh
sudo apt remove sidera
```

该命令移除系统安装包，不承诺清除用户设置、WPS 登记或已经安装到用户目录的 GNOME 扩展。需要停用扩展时，通过 GNOME 扩展管理器停用“Sidera 原生批注”；清理 WPS 登记时只移除 `sidera-bridge` 条目并保留其他加载项。
