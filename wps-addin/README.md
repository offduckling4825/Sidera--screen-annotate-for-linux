# Sidera WPS 加载项

加载项把 WPS 的放映位置、文档页码和状态回传给批注程序，使批注只在真换页时切换，页内动画不切缓存。内容由 Sidera 的 `http://127.0.0.1:16666/` 提供，在设置中开启“WPS 联动”。

## 部署与使用

关闭 WPS，以普通用户运行一次登记：

```sh
bash install.sh
# 安装 DEB 后：
sidera-wps-addin-install
```

先启动 Sidera，再打开 WPS 演示。出现“批注联动”标签表示加载项已加载；“状态”按钮会询问桥的真实健康响应，连接失败不会再显示在线。修改或更新脚本后需重新打开 WPS。

安装器需要 Python 3，只合并本项目节点，迁移旧名称，保留其他登记、属性和注释；修改前备份、同目录原子替换，异常 XML/符号链接/写入失败不会覆盖原文件。Rust、C++ 自动登记和手动命令共用该脚本，通过文件锁协调本项目并发写入。锁不能约束外部修改器，因此仍建议退出 WPS 后登记。

| 状态 | 当前 Rust 版行为 |
| --- | --- |
| 活动文档支持“当页开始” | 绿色“开始放映”，在执行时从当前页开始 |
| 联动放映中 | API 上下翻页、“结束放映”；API 失败不补发一次键盘 |
| 未联动/断线 | Esc 和上下键按桌面直接输入能力模拟；不可用时变橙，不发起共享桌面授权 |
| 有真实页码 | 显示当前页/总数；点击拨轮预选，再确认跳转 |
| 页码均未知/未联动 | 整块页码隐藏；仅缺一项时显示该项为“—” |
| 白板 | 本地 10 页，外部状态继续跟踪，退出恢复透明批注页 |

C++ 旧版共用此加载项与新桥，但界面仍以 NEXT/PREV/EXIT 为主，没有同步 Rust 的开始按钮、页码拨轮和 GNOME 原生能力。

## 状态与命令

保留兼容的 GET 协议：

- `/hello`：健康问候，不标记放映已联动。
- `/push?m=EVENT ...`：事件和状态上报。
- `/poll?m=EVENT SlideShowState ...`：完整快照及一条待执行命令；该有效轮询才刷新在线状态。
- 静态白名单：manifest.xml、index.html、main.js、js/bridge.js、ribbon.xml；未知资源返回 404。

`pos` 是放映位置，用于批注分页；`page` 是 Slide.SlideIndex，`total` 是 Slides.Count，不用显示页码替代放映导航位置。自定义放映的两种编号可能不同。旧加载项缺少新字段时保持基本翻页，不伪造总页数。

`ready` 表示支持当前页开始；START 调用 `CommandBars.ExecuteMso("SlideShowFromCurrent")`，存在 GetEnabledMso 时先核对。不可用时不退回从头 Run()。GOTO 带预选页与文稿标记 `deck`，执行前再次核对，避免旧拨轮跳转到新课件。

新脚本在 push/poll 上增加 `sid` 与递增 `seq`，同一页面重载脚本继续使用原流；服务端丢弃该流倒序/重复请求。API 对象被替换时重新注册监听，旧对象回调停止发布。未携带序号的旧客户端仍兼容，但没有这些顺序保证。

## 重试与边界

状态上报收到成功 HTTP 和 OK 才确认；失败/3 秒超时后重试最新快照，等待期间合并变化，迟到回复不能覆盖新状态。轮询每 300ms 调度，最多一个逻辑在途请求；支持 AbortController 时取消超时请求。服务端命令队列最多 8 条、有效期 1.5 秒，断线、结束放映或白板切换清待执行命令。

HTTP 请求头最多 8 KiB、请求行最多 4 KiB，有读取时限、Host/Origin 和方法校验；只监听回环地址。**sid/seq 和回环地址不是身份认证**，多加载项竞争、完整放映会话及命令 ID/执行确认仍见 [S12](../docs/ISSUES.md#s12--http-会话与资源边界不足)。不要转发端口到外网。

## 文件与卸载

加载项文件为 manifest.xml、ribbon.xml、main.js、js/bridge.js 和 index.html；publish.xml 是登记模板。登记位置为 `~/.local/share/Kingsoft/wps/jsaddons/publish.xml`。卸载时先关闭 WPS，只删除 sidera-bridge 条目，保留其他项目。

普通 DEB 不修改 WPS OEM 开关；仅在已明确诊断需要时使用仓库的 enable-jsapi.py，并保留其备份。

## 验证

在仓库根运行：

```sh
node --test wps-addin/tests/bridge.test.cjs wps-addin/tests/state-sync.test.cjs
python3 wps-addin/tests/test_install.py
python3 cpp/tests/test_bridge_api.py
python3 tools/test_rust.py wps::
```

Node 使用可控 Application、网络和定时器，不启动真实 WPS；安装器和桥检查在临时用户目录/私有网络执行。缺依赖或隔离失败即报错。最新结果见 [复查报告](../docs/audit/REVIEW_2026-10-24.md)。

历史 custom.4 在当前 VMware 上的基本联动曾得到用户“成功了”的确认，见 [历史记录](../docs/audit/WPS_INTEGRATION_2026-10-24.md)。本轮对请求边界和 API 生命周期的修改仍需实际 WPS 验收，不能把模拟通过写成所有 WPS 版本兼容。
