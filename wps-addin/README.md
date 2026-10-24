# Sidera - WPS 加载项部署包

作用：让 WPS 演示(放映)期间，把“真实页号变化”回传给批注 app，
使得批注缓存只在**真换页**时切换；页内动画步不动批注。
app 侧需开启 `设置 → WPS 联动`（内容由此服务的 127.0.0.1:16666 提供）。

## 部署（每台机器一次，按当前用户）
```bash
./install.sh            # 或安装 deb 后用: sidera-wps-addin-install
```

安装工具需要 Python 3（DEB 已声明依赖）。它只新增或更新本项目的登记，将旧名称 `screen-annotate-bridge` 合并为 `sidera-bridge`，保留其他加载项、根节点属性、注释和扩展节点。重复安装无需改写已正确的登记。

修改现有文件前会在同目录保存 `publish.xml.sidera-backup-*` 原文件备份，再原子替换登记文件；损坏或不支持的 XML、符号链接以及写入失败会报错并保留原文件。请先关闭 WPS 再运行安装工具，完成后重新打开。本行为适用于上述安装命令；应用内部自动登记的剩余问题见 [S11](../docs/ISSUES.md#s11--wps-登记存在覆盖和迁移风险)。

## 使用顺序
1. 启动 Sidera app，并在设置里开启“WPS 联动”（或 WPS_API_DEBUG=1 启动）。
2. 再打开 WPS 演示 → 工具栏出现“批注联动”标签即加载成功（可点“状态”确认）。
3. 当前 Rust 版识别到有幻灯片的活动文档且支持 WPS“当页开始”命令时，侧栏显示绿色“开始放映”，点击后从当前查看的幻灯片开始；也可在 WPS 使用 Shift+F5。放映中点 ▼/▲ 推进，观察批注是否只在真实换页时切换。
4. 放映中按钮显示“结束放映”。联动未接入时为“Esc”，上下键和 Esc 使用后端的直接模拟键盘；模拟路径无法区分页内动画，页号按旧行为估计。没有免弹窗输入能力时只变橙色，不启动共享桌面授权。
5. 联动取得页码数据时，两个翻页箭头之间显示“当前页 / 总页数”。编辑和放映都能更新；自定义放映使用幻灯片在文档内的页码。只有一项数据缺失时该项显示“—”；未联动、断线、关闭联动或两项数据均未知时整块隐藏并收回空位，不把模拟按键次数当作真实课件页码。白板始终显示自身当前页及可用的 10 页。

页码显示新增可选的 `page=`、`total=` 状态字段，与原有放映位置 `pos=` 分开，不改变批注缓存或翻页行为。`page` 来自当前 View 的 `Slide.SlideIndex`，`total` 来自对应 Presentation 的 `Slides.Count`；旧加载项没有这些字段时仍可翻页，但不显示页码栏。升级此脚本后，请保存文档，退出 Sidera 和 WPS，先打开 Sidera 再打开 WPS，以加载新版联动脚本。

`EVENT SlideShowState` 新增 `ready=0|1`，旧加载项未发送时按不支持启动处理。`START` 调用 `Application.CommandBars.ExecuteMso("SlideShowFromCurrent")`，由 WPS 在执行时确定当前页，不缓存准备状态时的页号，也不修改文档的放映范围。存在 `GetEnabledMso` 时会在探测和执行前检查命令可用性；不支持时不退回从头放映。服务器和加载项均限制重复启动。已接入的放映如果 API 执行失败，不再额外发送键盘事件，避免一次点击翻两页。新行为的操作验收由用户进行，见 [当页开始记录](../docs/audit/WPS_CURRENT_SLIDE_2026-10-24.md)；此前行为见 [触摸界面记录](../docs/audit/TOUCH_UI_2026-10-24.md)。

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

页号轮询收到 HTTP 成功状态和 `OK` 回复后才记为已同步；失败后在下一轮重试当前状态。等待回复时合并连续变化，3 秒超时后恢复重试，忽略旧请求的迟到回复；支持 AbortController 时同时取消超时请求。已补充 START 能力探测与命令回归源；本轮按用户要求未执行测试。无 AbortController 时不能取消底层网络请求；服务端乱序、桥重启后同页同步、放映 Begin/End 回退及状态按钮误报仍见 [S10](../docs/ISSUES.md#s10--js-状态上报丢失和轮询生命周期缺失)。

安装测试需要 Linux、Python 3 和 bubblewrap，通过临时家目录和隔离网络运行真实安装脚本，覆盖登记保留、迁移、重复安装、备份、异常 XML、只读目录和替换失败。缺少依赖或隔离失败会直接报错，不会改用真实家目录测试。
