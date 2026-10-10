# Sidera - WPS 加载项部署包

作用：让 WPS 演示(放映)期间，把“真实页号变化”回传给批注 app，
使得批注缓存只在**真换页**时切换；页内动画步不动批注。
app 侧需开启 `设置 → WPS 接口调试模式`（内容由此服务的 127.0.0.1:16666 提供）。

## 部署（每台机器一次，按当前用户）
```bash
./install.sh            # 或安装 deb 后用: sidera-wps-addin-install
```

安装工具需要 Python 3（DEB 已声明依赖）。它只新增或更新本项目的登记，将旧名称 `screen-annotate-bridge` 合并为 `sidera-bridge`，保留其他加载项、根节点属性、注释和扩展节点。重复安装无需改写已正确的登记。

修改现有文件前会在同目录保存 `publish.xml.sidera-backup-*` 原文件备份，再原子替换登记文件；损坏或不支持的 XML、符号链接以及写入失败会报错并保留原文件。请先关闭 WPS 再运行安装工具，完成后重新打开。本行为适用于上述安装命令；应用内部自动登记的剩余问题见 [S11](../docs/ISSUES.md#s11--wps-登记存在覆盖和迁移风险)。

## 使用顺序
1. 启动Sidera app，并在设置里开启“WPS 接口调试”（或 WPS_API_DEBUG=1 启动）。
2. 再打开 WPS 演示 → 工具栏出现“批注联动”标签即加载成功（可点“状态”确认）。
3. F5 放映：点批注侧边栏 ▼/▲ 推进，观察批注是否只在真实换页时切换。

## 文件说明
- manifest.xml / ribbon.xml / main.js / js/bridge.js ：加载项本体
- publish.xml ：登记模板（install.sh 合并到 ~/.local/share/Kingsoft/wps/jsaddons/publish.xml）
- 内容由 app 的 HTTP 服务(16666)实时提供，无需额外服务器

## 卸载
删除 ~/.local/share/Kingsoft/wps/jsaddons/publish.xml 中对应条目即可。

## 开发验证

在仓库根目录运行：

```bash
node --test wps-addin/tests/bridge.test.cjs wps-addin/tests/state-sync.test.cjs
python3 wps-addin/tests/test_install.py
```

测试使用 Node.js 24 内置测试工具模拟 WPS 事件、网络和定时器，不连接真实 WPS 或本地桥服务。覆盖重复加载、接口延迟就绪、部分注册失败与重试；同一加载项页面内每种事件只注册一次，轮询与启动重试各最多保留一个。真实 WPS 的加载/卸载生命周期仍需实机验收。

页号轮询收到 HTTP 成功状态和 `OK` 回复后才记为已同步；失败后在下一轮重试当前状态。等待回复时合并连续变化，3 秒超时后恢复重试，忽略旧请求的迟到回复；支持 AbortController 时同时取消超时请求。当前共 21 项 JS 回归。无 AbortController 时不能取消底层网络请求；服务端乱序、桥重启后同页同步、放映 Begin/End 回退及状态按钮误报仍见 [S10](../docs/ISSUES.md#s10--js-状态上报丢失和轮询生命周期缺失)。

安装测试需要 Linux、Python 3 和 bubblewrap，通过临时家目录和隔离网络运行真实安装脚本，覆盖登记保留、迁移、重复安装、备份、异常 XML、只读目录和替换失败。缺少依赖或隔离失败会直接报错，不会改用真实家目录测试。
