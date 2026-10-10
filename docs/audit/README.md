# 2026-09-30 评审诊断

基线：`custom @ 0642103ae9d2067efef0be4f15bb515c7aa56e1c`。下述初次评审结果属于该基线；脚本读取运行时的源码并记录行为，不是发布验收测试。

## JavaScript 探针

```bash
node docs/audit/check_addin.cjs
```

加载指定 bridge.js（默认工作区版本），在 Node VM 中模拟 WPS Application、网络和定时器，不访问真实 WPS 或 HTTP 服务。初次评审 Node 版本为 24.19.0。

初次评审观察结果：

| 场景 | 期望 | 当前观察 |
| --- | --- | --- |
| startBridge执行两次 | 6个事件监听 | 12个；interval仍为1个 |
| 无ApiEvent：开始→结束→同页开始 | 3个状态转变都有通知 | 只推送1个SlideShowState |
| push失败后同页重试 | 至少2次尝试 | 1次 |
| 网络失败时点状态 | 显示实际连接状态 | 仍提示“批注联动桥在线” |

可传入源路径：`node docs/audit/check_addin.cjs /path/to/bridge.js`。输出中expected是期望，observed是现状；退出0仅表示探针完成。

2026-10-01 后续验证：S09 修复后，同一探针观察到 6 个监听和 1 个 interval，其余 S10 现象仍存在。启动行为的正式回归测试为 `node --test wps-addin/tests/bridge.test.cjs`，当前 8 项通过；与本探针的诊断退出码区别对待。

2026-10-10 后续修复：S10 的轮询页号确认/失败重试已有正式回归 `node --test wps-addin/tests/state-sync.test.cjs`，13 项通过，连同 S09 共 21 项 JS 回归通过。探针现使用带 HTTP 状态的响应和完整异步等待，观察失败重试恢复；Begin/End 缺失和离线状态误报仍存在。客户端超时/迟到回复保护不代表服务器已具备会话与执行顺序保证。

## C++ 独立桥探针

```bash
python3 docs/audit/check_cpp_bridge.py
```

要求Linux、Python3、GCC、pkg-config、Qt5Core/Network开发依赖及bubblewrap。脚本：

1. 在临时目录编译 `bridge_probe.cpp` 和仓库原始 `cpp/src/wps_bridge.cpp`。
2. 用bubblewrap映射临时家目录并隔离网络，生产16666端口与原WPS登记不受影响。
3. 验证缺manifest时的响应、原Python套件的判断、启停timer和放映回调。
4. 在隔离目录预置其他加载项，执行原install逻辑。临时副本仅规范LF并保留相邻publish.xml查找方式，未更改安装语义。

本轮结果：

```json
{
  "missingManifest": { "status": 200, "body": "OK" },
  "existingSuiteWithMissingManifest": { "exitCode": 0, "reportedFivePasses": true },
  "pollAfterExistingSuite": { "status": 200, "body": "" },
  "callbacksAndTimers": ["ACTIVE_TIMERS 3", "BEGIN 1", "BEGIN 3"],
  "installerPreservesOtherAddin": false
}
```

已有套件取走探针预先入队的NEXT，之后poll为空；它没有对命令顺序做有效断言。回调序列包含套件发送的Begin1和探针发送的Begin3；之后End/State3不触发独立桥结束或新页回调。

该组件是C++ Wayland使用的桥，不代表完整Wayland图形后端或X11旧桥已经通过测试。脚本清除子进程的WPS_ADDIN_DIR，并在系统安装目录存在时把空目录映射到隔离视图，保证缺资源场景可重复；原系统文件保持原样。

2026-10-01 后续验证：S15 的 timer、连接与重启状态清理已修复，正式验收入口为 `python3 cpp/tests/test_bridge_lifecycle.py`，5 项通过（含 100 次启停）。上面的 3 个活动 timer 是初次评审结果；原接口测试误通过与登记覆盖问题仍分别按 S13/S11 跟踪。

2026-10-01 后续验证：S11 的独立安装脚本已修复，正式回归 `python3 wps-addin/tests/test_install.py` 共 13 项通过，覆盖其他加载项保留、迁移、备份和失败时原文件保留。上面的 `installerPreservesOtherAddin: false` 属于初次评审；Rust/C++ 应用内登记仍按 S11 跟踪。

## 构建与静态检查结果

- Ubuntu26.04.1/WSL2 amd64，GCC15.2.0、Qt5.15.18：C++ X11构建成功，存在Qt弃用警告。Wayland开发依赖缺失，所以未编入Wayland。
- 本轮构建的ELF含Qt5Network，最大GLIBC符号2.34和Qt_5.15符号；只用于诊断，不作为低ABI发布包。
- 原始main.js与bridge.js语法检查通过。
- 10个XML可解析；10个Shell脚本在LF规范化副本中bash语法通过。当前Windows检出的Shell为CRLF、Git索引LF；直接交给Bash有语法错误。
- Rust源有9个测试函数；未运行Rust工具链或图形/外设/arm64/DEB测试。

正式回归测试需将上述期望转成断言，使用可注入路径/端口，失败非零退出，并接入CI。问题编号、环境矩阵和验收标准见上级文档。
