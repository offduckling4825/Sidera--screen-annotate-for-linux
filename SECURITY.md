# 安全政策 / Security

本仓库是 Sidera 的课堂定制分支。安全问题与普通体验反馈使用不同入口，以免在公开讨论中暴露敏感信息。

## 私密报告

请使用 GitHub 的 **[Report a vulnerability](https://github.com/shi-zhe-lei/Sidera--screen-annotate-for-linux/security/advisories/new)** 提交私密报告。

请说明受影响版本 / 提交、桌面环境、影响范围和最小复现。日志与附件请删除密码、令牌、真实课件内容和学生信息。不要在公开 Issue 发布漏洞利用细节。

如果 GitHub 的私密报告入口暂时不可用，可先提交一个只询问联系方式的 Issue，**不附漏洞细节**。这是社区维护项目，目前不承诺固定响应时间、修复时限或漏洞奖励。报告流程参考 [GitHub 官方说明](https://docs.github.com/en/code-security/how-tos/report-and-fix-vulnerabilities/report-privately)。

## 维护范围

| 范围 | 处理方式 |
| --- | --- |
| `custom` 分支及最新 custom 预发布 | 当前接收问题、优先核实并改进 |
| 较早 custom 包 | 请先说明能否在最新版本复现；没有独立长期支持分支 |
| 上游 C++ 2.6 / 上游发行版 | 本仓库保留来源；不宣称已完成独立安全维护 |
| 系统图形服务器、WPS、第三方依赖 | 区分组件归属，必要时与对应上游协调 |

预发布状态和可编译不等于安全审计通过。

## 权限与已知边界

- **桌面与输入**：Sidera 是覆盖层应用，需要接收绘图输入、模拟部分控制按键，并在用户操作时截图。请在普通桌面用户下运行，不以 root 启动应用。
- **WPS 本地桥**：监听 `127.0.0.1:16666`。回环地址不等于身份认证，目前仍有会话、跨域与资源边界待加固，见[问题清单 S12](docs/ISSUES.md#s12--http-会话与资源边界不足)。不要将端口转发到外部网络。
- **GNOME 扩展**：运行于用户 GNOME Shell，使用用户运行时目录中的受限 socket，并校验同一 UID。它不是用于隔离恶意同用户进程的安全边界。实现与协议见[扩展文档](compat/gnome-extension/README.md)。
- **按键模拟**：DEB 包安装 uinput 权限规则；可用的桌面后端可能借此注入输入。部署到多人共用设备前，应结合学校的账户和设备权限政策评估。
- **日志与截图**：诊断可能包含用户名、文件路径与显示环境；截图可能包含桌面内容。分享前脱敏。应用退出后，内存中的白板内容不会自动保存。
- **系统补丁**：`compat/xwayland-24.1.10/` 是特定 Ubuntu 环境的回补材料，不能当作跨发行版安全更新使用。

## 下载与部署

从本仓库的 [Releases](https://github.com/shi-zhe-lei/Sidera--screen-annotate-for-linux/releases) 获取包，核对架构、系统库要求和 SHA256。校验值用于核对文件一致性；本次 release 没有单独的数字签名。不要为了安装绕过依赖检查。

发现涉及多个组件的问题时，请在报告中区分主程序、WPS 加载项、GNOME 扩展和系统图形栈，便于定位与协调修复。
