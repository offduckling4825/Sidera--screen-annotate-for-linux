# 审查与历史记录

当前代码状态以 [问题清单](../ISSUES.md)、[项目分析](../PROJECT_ANALYSIS.md)、[重构计划](../REFACTOR_PLAN.md) 和 [测试环境](../TEST_ENVIRONMENT.md) 为准。

## 最新复查

[2026-10-24 旧问题与全 Markdown 复查](REVIEW_2026-10-24.md)：重新核对 S01–S28，修复确认的代码缺陷，执行获准的隔离检查。没有操作当前 VMware 桌面、真实 WPS 或用户配置。

## 历次实施记录

这些文件保存当时版本、安装哈希、日志位置与当时授权范围。“未提交”“仅编译”“当前已安装”等句子只描述对应阶段，不代表仓库现状；测试允许范围及剩余任务以最新复查为准。临时文件路径是证据位置，不保证永久存在。

| 记录 | 主要版本/主题 |
| --- | --- |
| [VMware 初查](VMWARE_COMPATIBILITY_2026-10-11.md) | 最初安装/启动判断、Wayland 缺协议与显示服务器崩溃 |
| [兼容修复](COMPATIBILITY_2026-10-24.md) | custom.1–.2；Xwayland 回补、分层穿透与用户确认 |
| [原生 Wayland](NATIVE_WAYLAND_2026-10-24.md) | custom.3；Sway 基础验证及可切换测试桌面 |
| [WPS 联动](WPS_INTEGRATION_2026-10-24.md) | custom.4；免授权翻页与用户确认 |
| [GNOME 原生](GNOME_NATIVE_2026-10-24.md) | custom.5；扩展与初次加载问题 |
| [触摸 UI](TOUCH_UI_2026-10-24.md) | custom.6；后被替代的外观及放映逻辑 |
| [标准启动](GNOME_STARTUP_2026-10-24.md) | custom.7；GNOME 50 参数修正 |
| [当前页开始](WPS_CURRENT_SLIDE_2026-10-24.md) | custom.8 |
| [拖动释放](DRAG_RELEASE_2026-10-24.md) | custom.9；后续抓取见课堂记录 .28 |
| [白板保留](WHITEBOARD_RETENTION_2026-10-24.md) | custom.10 / .12 |
| [课堂界面迭代](CLASSROOM_UI_2026-10-24.md) | custom.11–.30；完整历史，后文覆盖旧规则 |

## 初评诊断材料

2026-09-30 基线为 `0642103`。当时发现 JS 重复监听、上报失败不重试、错误在线提示，以及 C++ 缺 manifest 仍返回成功、旧接口套件误通过、登记覆盖等问题；这些已经转入当前回归和问题清单，不再把旧观察当作当前行为。

`check_addin.cjs`、`check_cpp_bridge.py` 与 `bridge_probe.cpp` 保留为初评材料。它们是诊断探针，退出 0 只代表运行结束，不是发布门禁；旧桥 API/测试接口可能已变化，不作为当前版本的推荐测试命令。

当前使用 `wps-addin/tests/`、`cpp/tests/`、`rust/tests/` 和 `tools/test_rust.py` 的正式检查。旧的 5/5 mock/接口结果、20 页缓存限制、两个独立 C++ 桥及“custom 不触发 CI”等描述均已过时，详见最新复查。
