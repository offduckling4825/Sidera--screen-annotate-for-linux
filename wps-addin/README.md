# Sidera - WPS 加载项部署包

作用：让 WPS 演示(放映)期间，把“真实页号变化”回传给批注 app，
使得批注缓存只在**真换页**时切换；页内动画步不动批注。
app 侧需开启 `设置 → WPS 接口调试模式`（内容由此服务的 127.0.0.1:16666 提供）。

## 部署（每台机器一次，按当前用户）
```bash
./install.sh            # 或安装 deb 后用: sidera-wps-addin-install
```
请先编译或安装新版 Sidera。脚本调用 `sidera --register-wps-addin`（源码目录优先使用本机架构的构建产物），无需启动桌面窗口。
手动安装和 app 自动注册共用同一逻辑：保留已有加载项，仅在缺少时追加 Sidera；已登记的旧名称 `screen-annotate-bridge` 也不会重复添加。
已有 `publish.xml` 为空、损坏或无法读取时会报错并保留原文件，请检查后重试。保存使用原子替换，写入失败不会截断原文件。

## 使用顺序
1. 启动Sidera app，并在设置里开启“WPS 接口调试”（或 WPS_API_DEBUG=1 启动）。
2. 再打开 WPS 演示 → 工具栏出现“批注联动”标签即加载成功（可点“状态”确认）。
3. F5 放映：点批注侧边栏 ▼/▲ 推进，观察批注是否只在真实换页时切换。

## 文件说明
- manifest.xml / ribbon.xml / main.js / js/bridge.js ：加载项本体
- publish.xml ：登记格式示例；安装程序合并更新 ~/.local/share/Kingsoft/wps/jsaddons/publish.xml
- 内容由 app 的 HTTP 服务(16666)实时提供，无需额外服务器

## 卸载
删除 ~/.local/share/Kingsoft/wps/jsaddons/publish.xml 中对应条目即可。

## 回归测试
在项目根目录执行 `make test-registration`，使用临时用户目录测试注册，不需要 WPS 或桌面会话，也不会修改当前用户的 WPS 配置。
