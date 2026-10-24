# 课堂浅色界面设计与实施

> 历史阶段记录：以下版本、安装状态、测试限制与“未提交”等表述仅指当时。当前代码和隔离检查见 [最新复查](REVIEW_2026-10-24.md)，当前使用规则见 [教室指南](../../README.classroom.md)。

日期：2026-10-24；分支 `custom`。用户选择浅色课堂风格，要求认真构思、重画画笔和橡皮。追加要求：笔尖跟随当前画笔颜色，橡皮图标随尺寸档位变化，但不能过小或过大。沿用此前的位置不变、禁用悬浮反馈、增强点击反馈，以及交互由用户测试的要求。

## 设计取舍

历史规则提示：`.14` 替代浅色选中态；`.19` 改为未联动隐藏页码；`.20` 修正白板清除保持展开；`.28` 替代越界拖动和文字气泡；`.30` 增加返回有笔迹页自动展开。请勿将下文旧版过程当作现行操作说明。

原界面的问题是按钮底色、形状、边框和图标笔触各不相同；画笔只由两段线组成，橡皮像直立色块；选中态和普通动作混用高饱和颜色。此次使用同一套圆角、线宽和有限的状态色，让教学内容仍是画面主体。

- 暖白面板 `#FAFBF7`，深灰绿文字 `#263E3A`，蓝绿主色 `#216F62`。选中态使用浅蓝绿底和清晰轮廓；普通动作使用中性面板。开始放映保留绿色，结束与清空采用低饱和红色，翻页不可用仍只改变颜色。
- 画笔、光标、橡皮、翻页、更多、截图和设置使用共用 24 单位矢量网格及圆角笔触，不依赖系统符号字体。画笔有完整笔杆、笔帽和笔尖，笔尖实时使用 `App::pen_color()`；浅色笔尖仍有深色外轮廓。橡皮采用倾斜的双色文具造型。
- 橡皮小/中/大档仅使用 `0.88 / 1.00 / 1.12` 的视觉比例。默认工具栏图标约 `21 / 24 / 27` 逻辑像素，上限为原按钮短边的 82%；中心、按钮尺寸和点击区域不变。不会按实际擦除宽度做 4 倍图标缩放。
- 色盘使用选中环及勾选形状，显示当前颜色名称；粗细选项画出对应线宽。橡皮菜单里的“清除全部”改为符合原功能范围的“清空当前页”，行为不变。
- 设置页加强标题层次，将相邻开关和辅助操作分组，减少逐行方框；标签字号增大，滑条数值使用固定位置的数值块。不透明度和工具栏大小显示百分比，擦除倍率显示乘数，底层范围和数值不变。更多菜单、确认、诊断和启动画面使用同一配色，作者信息保留。
- 软件图标保留原标志的左侧刻度、中间 S 曲线、右侧轮廓及构图，只适配暖白与蓝绿配色。原 `sidera.svg` 实际嵌入位图，因此本次继续保存自包含的位图 SVG，并导出 256×256 PNG；没有把它描述成重新绘制的矢量标志。
- 按下及快速点击保留 160 ms 的反馈，使用明显的蓝绿描边与压暗；没有添加悬浮效果、鼠标追踪动画或控件位移。面板装饰限制在现有绘制边界内，避免局部重绘留下残影。

代码把调色板、矢量图标和启动画面纯绘制拆为小模块，供生产界面和静态预览共用。没有改变绘图颜色、白板背景色、工具栏布局或功能路由。

## 静态预览与验证边界

`rust/examples/render_ui.rs` 只创建内存画布，调用生产绘制函数生成 PNG；没有桌面后端、应用事件循环、WPS 连接或输入注入。预览用独立配置目录，不更改用户设置。生成内容包括工具栏状态、10 种笔尖颜色、三档橡皮、面板、浅深背景和不同工具栏/显示缩放。

按用户要求不运行测试用例或虚拟机交互。已使用 VM 的 `NotoSansCJK-Regular.ttc` 在独立构建环境中生成预览并目视检查中文、10 种笔尖颜色、三档橡皮、深浅背景、0.6/1.0/1.4 工具栏尺寸以及 1.25/2.0 显示缩放；没有安装字体到系统或改变生产字体选择。第一轮预览中笔尖着色面积过小，已扩大到笔尖前段，使默认尺寸下的颜色可辨认。

`cargo check --tests --examples --locked` 通过，仅编译、未执行测试。20 个原几何/命中函数源码保持一致。渲染图片不代表真实触屏、穿透或 WPS 操作通过。最终构建和安装状态在下文记录。

## 图标编辑来源

使用 `imagegen` 内置编辑模式，以原来的 `sidera.png` 为目标图，不使用 CLI/API 回退。编辑要求是“保留原构图、原线条结构和比例，仅将黑色霓虹质感调整为暖白底、低饱和深蓝绿色标志与柔和边缘”。原图本身不透明，输出沿用不透明背景；PNG 尺寸导出和 SVG 封装不改变生成图的构图。

项目资源为仓库根目录 `sidera.svg` / `sidera.png`，应用菜单、兼容入口和软件内显示继续沿用现有图标引用。原图备份在宿主临时目录的 `classroom-before/`，生成源保存在 Codex `generated_images/01a0f289-5425-70b0-ae51-d22ebeb00167/exec-7ffb242a-f294-498a-8fbd-636e9341667d.png`；安装包包含项目内资源，不依赖该生成目录。

### 内置编辑提示词

内置图片编辑使用的最终提示词：

```text
Edit the supplied existing Sidera application icon. This is a strict recolour and material/style adaptation of the ORIGINAL icon, NOT a logo redesign.
Keep the exact original composition, outer rounded-square tile location and corner radius, scale, margins, and every stroke and negative space of the original abstract mark: the left vertical frame and its three small horizontal ticks, the nested central S curves, and the right bracket/corner frame. Do not replace them with a simple S. Do not add a pencil, board, stand, badge, letters, words, or new elements. The contours and stroke geometry must match the reference.
Change only the palette and surface lighting to coordinate with a calm light classroom application UI: warm off-white background and tile (#FAFBF7 / #F0F4EF); clearly readable dark desaturated teal mark (#216F62), with a very restrained sage-to-teal variation if needed. Make the tile outline soft sage grey (#CBD7CE). Remove the black backdrop, neon glow and strong glassy reflections; use a clean, quiet matte finish and only a very subtle edge shadow. Preserve the same recognizable original mark. High contrast, clean crisp edges, suitable for a school computer application icon, readable at small sizes.
Output a single square icon, keeping the same framing as the source. No mockup, no labels, no new brand, no extra decoration. The only content is the recoloured original icon.
```

## 构建与部署

`cargo build --release --locked` 通过，VM 已覆盖安装 `3.0-Electro-testing+custom20261024.11`，dpkg 为 `install ok installed`。主程序、scalable/pixmaps 两处 SVG 和 256×256 PNG 的安装后校验和均与制品一致，桌面菜单与图标缓存更新触发器正常完成。

| 制品 | SHA256 |
| --- | --- |
| `.11` amd64 DEB | `573e377e0484ed1bb48a9c099ae354b04b114f4d94fe98bf923334ff650ab657` |
| `/usr/bin/sidera` | `4a234c52a018cb8484ae4d2b0404f44fcbf8fb52f1c700cba5886887d8a327d6` |
| `sidera.svg` | `afe328fcece799b1c4437c0d6debf25919d63b961942ef54488e6d6ae036032c` |
| `sidera.png` | `9c778eeeef6c0ca31f0481eb5220dc55b61f1cb13a8cea6f6053b417582d9298` |

GNOME 扩展、WPS 加载项和白板保留逻辑沿用 `.10`。安装后只读确认 Shell PID 17662 已加载扩展版本 4 / ACTIVE，前一轮拖拽扩展已生效；这不等于拖动行为已验收。Sidera PID 19462 仍使用 `.10` 的旧程序映像，需由用户退出、重新打开加载新界面；本次无需再次注销，也未代替用户结束当前应用。

构建记录：WSL `/var/tmp/sidera-install-d8d04c7-68421ff3/classroom11-check.log`、`classroom11-build.log`、`classroom11-package.log` 和 `classroom11-artwork.log`。VM 记录：`/var/tmp/sidera-classroom11-install.log`、`sidera-classroom11-status.log`。本地包声明 `libc6 >= 2.43`，仅供当前 Ubuntu 26.04.1 VM；仓库通用打包版本/最低 libc 未改。

静态预览保存在宿主 `C:/Users/shi01/.codex/visualizations/2026/09/30/01a0f289-5425-70b0-ae51-d22ebeb00167/classroom-ui-20261024/`，包括总览、设置和浅深背景图。应用图标嵌入的是配色后的原标志，无新标志。

回退：VM `/var/backups/sidera-compat-20261024/` 保存 `.10` 包、`sidera-before-classroom11` 及升级前的 SVG/PNG。本轮未提交或 push。

## 后续：Ubuntu 菜单图标刷新（`.13`）

用户反馈应用菜单/底部栏仍是旧图标。只读检查确认 `.12` 的 SVG/PNG 已与新资源一致，当前 Sidera 进程也已使用 `.12` 主程序，没有用户级 desktop/icon 覆盖或 `SIDERA_ICON` 覆盖。三个有效入口仍引用固定绝对路径 `/usr/share/pixmaps/sidera.svg`；因此按桌面继续复用旧路径纹理的可能性修复，未把文件校验通过等同于桌面视觉已更新。

打包脚本现在为 SVG 增加包含内容摘要的路径，并让标准、GNOME 原生、兼容模式三个入口统一引用它。本次为 `/usr/share/pixmaps/sidera-afe328fcece7.svg`；后续图标内容改变时路径自动改变，desktop ID 不变，保留固定图标别名供其他引用使用。没有再次改动标志、配色或 Rust 程序。

已覆盖安装 `3.0-Electro-testing+custom20261024.13`，dpkg 状态正常。主程序继续为 `.12` 的 `c9b819711e7a46e1d74881f350cb42dc86f465db66144e098ab5941dfd597649`；DEB SHA256 为 `f696615e2bc6bfd34d5c5f81ab3c16f7b9e77aec47e8e77067bee42cc94729db`。打包脚本语法和 desktop 文件格式检查通过；从用户会话读取的 Gio 有效入口均已解析到新路径。未执行测试、启动程序或操作桌面，实际显示由用户重新展开应用菜单确认；没有注销或结束正在运行的 Sidera。

VM 记录为 `/var/tmp/sidera-icon13-audit.log`、`sidera-icon13-install.log`、`sidera-icon13-after.log`；WSL 记录为既有构建目录下 `icon13-package.log`。`.12` 包已备份至 `/var/backups/sidera-compat-20261024/`。本轮未提交或 push。

## 后续：加强工具选中状态（`.14`）

用户要求画笔、光标等选中与未选中的区别更明显。两侧工具栏的光标、画笔、橡皮和白板开启状态改用深蓝绿色实底、深色边框；未选中继续为暖白底。工具图标保留浅色填充和深色细节，避免白色线条在浅色笔身上消失，笔尖继续显示实际画笔颜色。白板禁用的光标仍为灰色且不响应点击。

选中按钮按下后使用亮色内边框与浅色反馈，避免深色反馈与选中底色混合。仅调整绘制与离线预览示例，没有改按钮尺寸、位置、命中区域、悬浮行为或功能逻辑。

`cargo check --tests --examples --locked` 通过，仅编译；没有执行测试、启动 Sidera 或操作虚拟机桌面。使用实际绘制函数生成静态状态对比，检查光标、画笔、橡皮、白板四种状态及笔尖色；原有 20 个布局/命中函数源码仍未变。预览为宿主 `C:/Users/shi01/.codex/visualizations/2026/09/30/01a0f289-5425-70b0-ae51-d22ebeb00167/classroom-selection-20261024/classroom-selection.png`，不是虚拟机实机截图。

发布构建通过，VM 已覆盖安装 `3.0-Electro-testing+custom20261024.14`，dpkg 状态为 `install ok installed`。DEB SHA256 为 `6df41271534811ee451ab3456cc316d8b4f06c3654d22990741bebb00585e31d`，主程序为 `314b7c83cfdeac877f97001d1631be2af85baf33e1b0471410a10ee691204bab`，安装后校验一致。图标、GNOME 扩展和 WPS 加载项校验未变。没有结束当前 Sidera PID 22524，由用户退出重开后确认显示效果，无需注销。

WSL 构建目录保存 `selection14-check.log`、`selection14-artwork.log`、`selection14-build.log`、`selection14-package.log`；VM 安装记录为 `/var/tmp/sidera-selection14-install.log`。回退目录保存 `.13` 包及 `sidera-before-selection14`。未提交或 push。

## 后续：绘图后工具弹窗破块残影（`.15`）

用户截图中，画笔/橡皮弹窗出现沿笔迹轨迹分布的透明矩形缺口，笔迹与弹窗残片重叠。源码定位到：在弹窗外按下鼠标开始绘图时，`close_all_popups` 只修改可见状态，随后仅刷新新笔迹附近的小区域；旧弹窗的其他像素留在后端画面中，继续绘图时才被逐块覆盖。这一缺少旧区域失效通知的路径与截图相符。

关闭弹窗现在先记录各面板的完整原区域，清除可见状态后刷新这些区域并更新输入区域；所有调用方通过同一个入口处理。触摸在弹窗外开始绘图时也关闭弹窗，点击弹窗内部空白仍不会落笔。绘制顺序、工具栏布局、GNOME 扩展和原有笔迹数据不变。

`cargo check --tests --examples --locked` 通过，仅编译。补充回归源覆盖左右弹窗、画笔/橡皮、鼠标/触摸起笔时完整刷新旧面板，以及面板内部空白不落笔；按用户要求没有执行测试用例、启动应用或操作虚拟机桌面。用户重新打开 Sidera 后，可展开画笔或橡皮面板，在面板外开始书写/擦除并经过原面板位置，确认整个面板已消失且没有残片。

发布构建通过，已覆盖安装 `3.0-Electro-testing+custom20261024.15`，dpkg 状态正常，主程序校验一致。DEB SHA256：`2709902cf120cfacab41a54107cf1c6b1d2818a300b6870a7142accae7862cd6`；主程序：`282516b24a940bdd6e1e6fda0cf89c426cd9f3108e5576157bd4dd43fcf9fbe4`。图标、GNOME 扩展与 WPS 脚本均沿用前版，扩展版本 4 / ACTIVE，无需注销。

只读检查确认安装前运行的 PID 22524 仍为 `.12` 的旧主程序（`c9b8197…`）；安装没有结束此进程。用户须退出重开才能加载 `.14` 的选中样式和 `.15` 的弹窗修复。WSL 构建目录保存 `popup15-check.log`、`popup15-build.log`、`popup15-package.log`；VM 的 `/var/tmp/sidera-popup15-status.log` 为安装前只读状态，`sidera-popup15-install.log` 为安装结果。回退目录保存 `.14` 包及 `sidera-before-popup15`。未提交或 push。

## 后续：贴边收起与保持收起（`.16`）

用户要求工具栏向边缘缩进去、只露一个小角，并明确不要自动弹出。右侧向右、左侧向左滑出，约 180 毫秒后只保留贴边箭头把手；默认可见尺寸为 24×64 逻辑像素，随侧栏缩放限制在宽 24–30、高 56–72 范围，兼顾遮挡面积和触摸点击。点击小角展开，展开后的按钮排列、尺寸和原侧栏位置保持原规则。

已删除旧的两分钟自动展开计时字段与周期任务。空闲、WPS 状态更新和动画结束都不会把工具栏自动打开；用户点击展开、使用绘图快捷键/单实例唤醒仍属于主动展开。收起前关闭弹窗；动画期间只更新边缘窄条，结束时把输入区域缩到把手，隐藏主体不继续挡住下方窗口。左右输出同步目标状态，快速切换从当前收缩进度继续。

`cargo check --tests --examples --locked` 通过。新增回归源覆盖空闲更新后保持收起、隐藏区域释放，以及左右把手通过鼠标/触摸展开；仅编译，没有执行测试或操作虚拟机。静态预览使用生产绘制代码，位置为宿主 `C:/Users/shi01/.codex/visualizations/2026/09/30/01a0f289-5425-70b0-ae51-d22ebeb00167/classroom-edge-tabs-20261024/classroom-collapsed.png`，不是实机截图。

本轮有意修改了收起几何；源码核对确认其余 18 个既有几何/命中函数及展开状态的按钮布局计算未变，不能继续使用此前“20 个函数全部未变”的口径。人工验收由用户完成：收起后等待超过两分钟应保持小角；点击左右任一把手展开；收起后原工具栏主体位置应能操作下层窗口。

发布构建通过，VM 已覆盖安装 `3.0-Electro-testing+custom20261024.16`，状态为 `install ok installed`，主程序校验一致。DEB SHA256：`143634f299e4b244c83f4a07f07f3f0e296793d1f9c898d1bf3bcf9b55f8ad44`；主程序：`dde9b1177deda71adb8c1f8dd2aa99922b8f3da47119adfd795e84d0ec4e5a3e`。GNOME 扩展、WPS 加载项和软件图标校验未变，无需注销。

安装未结束正在运行的 Sidera PID 24328，用户退出重开后加载新程序。WSL 构建目录保存 `edge16-check.log`、`edge16-artwork.log`、`edge16-build.log`、`edge16-package.log`；VM 安装日志为 `/var/tmp/sidera-edge16-install.log`，回退目录保存 `.15` 包及 `sidera-before-edge16`。未提交或 push。

## 后续：翻页按钮之间的页码（`.17`）

两侧工具栏在上一页与下一页之间新增“当前页 / 总页数”。原按钮顺序和大小保留，只为页码行增加所需高度；收起的小角尺寸、保持收起规则不变。正常宽度采用一行显示，较小工具栏或较长数字改用上下两行并按可用宽度排版，完整保留两个数字。

WPS 桥新增独立的 `page` / `total` 元数据：编辑状态从活动文档窗口读取当前 Slide，放映状态使用对应放映窗口及其 Presentation；`Slide.SlideIndex` 是文档页码，`Presentation.Slides.Count` 是文档总页数，不把自定义放映顺序当作文档页码。原 `pos` 与批注缓存、翻页和“当页开始”命令路径保持分离。依据为 WPS 官方 [Slide 对象](https://open.wps.cn/documents/app-integration-dev/wps365/client/wpsoffice/jsapi/wpp/Slide/obj)、[SlideShowWindow 对象](https://open.wps.cn/documents/app-integration-dev/wps365/client/wpsoffice/jsapi/wpp/SlideShowWindow/obj) 与 [Slides 集合](https://open.wps.cn/documents/app-integration-dev/wps365/client/wpsoffice/jsapi/wpp/Slides/obj)；当前 Linux WPS 的实际接口行为仍待用户验收。

没有联动、字段不支持或失联超过现有心跳期限时显示“—”；不会拿模拟按键的估算页号冒充真实页码。旧加载项仍可导航，缺失元数据时清掉旧显示。白板优先显示本地当前页及可用 10 页，退出白板等待新的 WPS 状态，不显示前一份课件的旧页码。更改页码/总页数会触发状态同步，单独的计数变化不产生批注换页事件。

`cargo check --tests --examples --locked` 和 `node --check` 通过。新增回归源覆盖编辑页码和总数变化、自定义放映与其他活动文档分离、未知字段、旧加载项、断线、白板优先，以及计数不能驱动批注缓存；只编译/检查语法，没有执行用例。静态图由生产绘制函数生成，检查了正常、未联动、白板、三位数和较小工具栏状态，位于宿主 `C:/Users/shi01/.codex/visualizations/2026/09/30/01a0f289-5425-70b0-ae51-d22ebeb00167/classroom-pages-20261024/classroom-pages.png`，不是实机截图。与 `.16` 的源码差异已核对，原导航命令和收起/展开处理未改。

人工验收由用户完成：保存文档，退出 Sidera 与 WPS，先开 Sidera 再开 WPS；在编辑与放映中翻页、增删文档页，确认页码随实际数据变化；关闭文档/断开联动后显示未知，进入白板显示白板页码。无需注销桌面。本轮没有启动应用或操作虚拟机桌面。

发布构建通过，VM 已覆盖安装 `3.0-Electro-testing+custom20261024.17`。首轮安装校验发现打包早于最后一处 JS 同步完成，脚本未包含最终修改；等待同步完成后重新打包并覆盖安装，最终程序、脚本、图标与 GNOME 扩展校验均通过。没有把首轮校验失败当作完成结果。

| 最终制品 | SHA256 |
| --- | --- |
| `.17` amd64 DEB | `b896b46fdef739b05a4ca51e8330f3031e274910148bacd9304831d5c5727602` |
| `/usr/bin/sidera` | `c96eabe93e768f6c39fb18429300761bab3c3a071bcd3d22ddf71d643df25e6a` |
| `/usr/share/sidera/wps-addin/js/bridge.js` | `b9d96d3d882e0c6970da49291b3479238b325c31dce5bdaf4bf2113349a687de` |

图标与 GNOME 扩展沿用 `.16`，不需要注销；没有结束正在运行的应用。WSL 构建目录保存 `pages17-check.log`、`pages17-artwork.log`、`pages17-build.log`、`pages17-package.log`；VM 最终安装日志为 `/var/tmp/sidera-pages17-install.log`，宿主临时目录另保留 `pages17-first-install.log`。回退目录保存 `.16` 包和 `sidera-before-pages17`。本地包继续要求 libc 2.43，仅适用于当前 Ubuntu 26.04.1 VM；仓库通用版本/最低 libc 未改。未提交或 push。

## 后续：绘图工具组按需展开（`.18`）

用户确认采用“收起绘图工具组，保留画笔和翻页入口”，而不是默认把整条工具栏藏到屏幕外。默认隐藏橡皮、清除和撤回；光标、画笔、上下翻页、页码、放映/白板及更多入口保留。展开后的顺序、按钮尺寸、配色和默认上端锚点沿用 `.17`。

| 操作 | 模式与工具组结果 |
| --- | --- |
| 启动 | 光标模式，绘图工具组收起 |
| 选画笔、主动通过快捷键进入绘图 | 选择画笔，工具组展开；首次不自动打开颜色面板 |
| 已展开时再点当前画笔/橡皮 | 开关对应颜色/大小面板，沿用原行为 |
| 切光标、翻页、WPS 状态变化、空闲 | 保持当前工具组开合状态，不自动收回或弹出 |
| 工具栏“清除”或橡皮面板“清空当前页” | 统一清除当前页及其旧缓存、关闭弹窗、收起工具组；透明批注回到光标 |
| 进入白板 | 自动选画笔并展开，记录进入前的工具组状态 |
| 白板内清除 | 清空当前页、收回工具组，保留白板与画笔；点画笔重新展开，首次不打开颜色面板 |
| 退出白板 | 保留白板内容，恢复进入前的模式与工具组状态 |
| 手动将整条工具栏贴边收起 | 绘图工具组同时收回，只留边缘把手；仍不会按时间自动展开 |

绘图工具组有独立状态和 220 毫秒缓动，连续反向切换从当前进度继续，无越界回弹。每帧统一采样开合进度，绘制与点击区域使用同一份几何；靠近底部时随高度增长平滑向上让位，收回时恢复原锚点。空间足够后完整按钮逐个淡入，不裁出半个图标/文字；下面的翻页区随工具组高度移动。动画期间暂不接受工具栏点击/拖动，避免命中移动中的其他按钮；结束后输入区域立即缩到可见工具栏。两侧及多显示器同步目标状态，保留各屏画布。

此前左右两条脏区最终会合并成覆盖整屏宽度的矩形。本次改为只合并相交区域，左右独立绘制；碎片数量设置上限，完整重绘仍覆盖所有区域。动画每帧只刷新工具栏移动范围，不截屏、不重建画布。调整工具栏大小时也刷新旧边界，避免缩小时残留旧背景。

`cargo check --tests --examples --locked` 通过，回归源仅编译，未执行测试。新增源覆盖默认精简、画笔展开、两种清除入口、白板恢复、快速反向、长时间空闲、近底部/多种缩放下的按钮边界、动画中不误触，以及左右脏区不合并成整屏宽度。静态分帧使用生产绘制函数，包含默认/展开中/展开后/清除后/白板清除后和底部展开的时刻；宿主预览目录为 `C:/Users/shi01/.codex/visualizations/2026/09/30/01a0f289-5425-70b0-ae51-d22ebeb00167/classroom-drawer-20261024/`。静态图不证明虚拟机帧率或触屏交互已通过，实际动画与操作由用户验收。

此前两个隔离图形回归脚本仍引用页码行加入前的工具栏坐标；本次同步到包含页码行的 490 像素完整布局，并给展开过程保留等待时间。仅使用 Python AST 解析检查语法，未运行这些脚本、启动图形测试环境或注入输入。

发布构建通过，VM 已覆盖安装 `3.0-Electro-testing+custom20261024.18`，dpkg 状态正常，程序与资源校验一致。DEB SHA256：`225295023b4e1fb0edb0aa5f567d84374013f5787b5efd8d5b8b70214560e71f`；主程序 SHA256：`17d79415314d35ee3dbf479f4054b28f722e8dd70ad62d196c8794a2a849d8f9`。WPS 脚本、软件图标和 GNOME 扩展保持原制品。没有结束正在运行的应用；由用户退出重开 Sidera 加载新逻辑，无需注销桌面。

WSL 构建目录保存 `drawer18-check.log`、`drawer18-artwork.log`、`drawer18-build.log`、`drawer18-package.log`；VM 安装日志为 `/var/tmp/sidera-drawer18-install.log`。回退目录保存 `.17` 包及 `sidera-before-drawer18`。本地包仍要求 libc 2.43，仅用于当前 Ubuntu 26.04.1 VM。未提交或 push。

## 后续：未联动隐藏页码（`.19`）

用户要求取消未联动时的“— / —”。现在未联动、断线、关闭联动或两个页码字段都未知时，整块页码栏隐藏并收回行高；上一页/下一页按钮直接相邻。白板保留本地页码；WPS 取得至少一项实际数据后恢复显示，只有单项缺失时保留该项的“—”。显示条件、工具栏高度及按钮位置共用一个判定。

显隐切换刷新侧栏旧、新边界并同步输入区域；需要调整位置的弹窗先按旧边界关闭，防止旧背景残留。直接模拟键盘路径检测到联动状态变化时也同步刷新。绘图工具组与整条工具栏的收起状态不由页码显隐改变。

`cargo check --tests --examples --locked` 通过；更新已有页码回归源中的可见性、行高和相邻箭头断言，以及原生 Wayland 隔离回归中的重设尺寸坐标。仅编译/解析，未执行测试。生产绘制函数生成的静态预览确认未联动列不再占位，位于宿主 `C:/Users/shi01/.codex/visualizations/2026/09/30/01a0f289-5425-70b0-ae51-d22ebeb00167/classroom-pagehide-20261024/classroom-pages.png`。未启动 Sidera、WPS 或操作桌面；实机联动与点击由用户验收。

发布构建通过，VM 已覆盖安装 `3.0-Electro-testing+custom20261024.19`，dpkg 状态正常，程序及资源校验一致。DEB SHA256：`8e3bfa97adae94a8b51bf9c235ed518ce81bef7a93e4801ca91879d832cd1f0e`；主程序：`14f8f0dd2e2c5bbdea2610408684039ec83efb4f9d2b217bca483f0810811f88`。WPS 脚本、GNOME 扩展与图标沿用前版。没有结束运行中的应用，由用户退出重开 Sidera 生效。

WSL 构建目录保存 `pagehide19-check.log`、`pagehide19-artwork.log`、`pagehide19-build.log`、`pagehide19-package.log`；VM 安装日志为 `/var/tmp/sidera-pagehide19-install.log`。回退目录保存 `.18` 包和 `sidera-before-pagehide19`。本地包仅用于当前 Ubuntu 26.04.1 VM，最低 libc 仍为 2.43。未提交或 push。

## 后续：空白页自动收回与白板清除修正（`.20`）

用户补充切回光标、翻页后满足条件也应收回，并指出白板清除不应收回。本节替代 `.18` 中对应旧规则：

| 操作 | 工具组结果 |
| --- | --- |
| 桌面批注切回光标（按钮、右键或快捷键） | 当前页无笔迹则收回；有笔迹则保留当前开合状态 |
| 桌面批注实际换页 | 光标模式且目标页无笔迹才收回；画笔/橡皮模式保持当前开合状态 |
| WPS 命令等待执行、页内动画、重复同页状态 | 不触发收回；实际新页到达后再判断 |
| 桌面批注清除 | 清空当前页和对应旧缓存，切光标并收回 |
| 白板内两种清除入口 | 清空当前页，保持画笔、绘图工具组展开；关闭工具弹窗 |
| 白板翻到空白页 | 继续保持绘图工具组可用 |
| 回到有笔迹的页面 | 不自动重新展开已收回的工具组；选画笔可展开 |

清除工具栏按钮与橡皮面板“清空当前页”共用修正后的处理。空白判断检查画布实际 alpha，不依赖擦除/撤回后可能仍为真的 `page_has_ink`；切光标先提交正在绘制的笔迹，避免把尚未提交的笔画判为空白。只在模式/页面明确变化时请求判断，不在闲置或动画每帧扫描画布。主动选画笔等操作撤销过时的收回请求。

X11 在本帧状态更新后处理请求；GNOME/原生 Wayland 在所有输出完成模式/换页同步后统一处理，任一活动输出仍有笔迹就保持工具组可用。收回同时更新共享控制状态，防止另一屏的后续事件重新展开；旧弹窗先按原边界关闭，输入区域随原有 220 毫秒动画更新。手动贴边收起规则不变。

`cargo check --tests --examples --locked` 通过。回归源覆盖两种清除入口的白板/桌面分支、空白/有笔迹/完全擦除/撤回/未提交笔画、空白缓存、翻页目的页、WPS 同页与换页、白板恢复，以及多输出一致性；仅编译，未执行用例、启动应用或操作虚拟机桌面。离线预览源同步修正白板清除示例；本轮没有重新渲染外观图。实际行为由用户验收。

发布构建通过，VM 已从 `.19` 覆盖安装 `3.0-Electro-testing+custom20261024.20`，dpkg 状态为 `install ok installed`，程序与资源校验一致。DEB SHA256：`0f91c4756ad5daa829d31c88e7b519c0d96cef77cee9504adc485efb020ff795`；主程序 SHA256：`6cc1104615648733f89ad2cfcd12671938a4389af17e30a640357c1c2a6c2bd7`。WPS 加载项、GNOME 扩展与图标沿用前版，无需注销桌面或重开 WPS。没有结束正在运行的 Sidera（安装记录 PID 27264），由用户退出重开加载新程序。

WSL 构建目录保存 `autofold20-check.log`、`autofold20-build.log`、`autofold20-package.log`；VM 安装日志为 `/var/tmp/sidera-autofold20-install.log`。回退目录保存 `.19` 包和 `sidera-before-autofold20`。本地包仍仅用于当前 Ubuntu 26.04.1 amd64 VM，最低 libc 2.43；仓库通用打包版本和最低 libc 未改。未提交或 push。

## 后续：工具栏大小精细调整（`.21`）

用户要求大小调整更细致。设置滑条步进从 `0.1` 改为 `0.01`，即 60%–140% 范围内每次 1%，可以选择 83%、97%、112% 等大小。滑条取值统一换算到整数百分比再参与布局，避免 `f32` 到 `f64` 的误差使同一百分比在拖动与重新读取配置后出现一像素偏差。配置原已按两位小数保存，可直接保留新精度；百分比显示、原滑条位置、触摸命中区域沿用现有实现。

`cargo check --tests --examples --locked` 通过，没有新增或执行测试，没有启动应用或操作虚拟机桌面；本轮也未生成新的外观预览。实际拖动调整和退出重开后的保留效果由用户验收。

发布构建通过，VM 已覆盖安装 `3.0-Electro-testing+custom20261024.21`，dpkg 状态正常，程序与资源校验一致。DEB SHA256：`494a2c74dee2ec8072b7be5a20f866ae3ef68f72ca8cbd1924ddc18101831355`；主程序 SHA256：`0f34ff3b19b9711970c43dd4e216c07ee02ee94a049368523b42c8b6643bdce1`。没有结束正在运行的 Sidera（安装记录 PID 27264），由用户退出重开生效。WPS 加载项、GNOME 扩展与图标校验未变。

WSL 构建目录保存 `scale21-check.log`、`scale21-build.log`、`scale21-package.log`；VM 安装日志为 `/var/tmp/sidera-scale21-install.log`。回退目录保存 `.20` 包及 `sidera-before-scale21`。本地包仅用于当前 Ubuntu 26.04.1 amd64 VM，最低 libc 2.43；仓库通用打包版本和最低 libc 未改。未提交或 push。

## 后续：取消拖拽上下限位、增加底部拖拽点（`.22`）

两侧工具栏保持左右贴边布局，上下位置直接跟随拖动，允许部分超出屏幕顶部或底部。移除拖动时的边界限制、绘制时的自动让位，以及大小调整/收起展开/多屏控制同步时的强制限位，避免松手后或切换工具组时跳回。此规则替代 `.18` 中靠近底部时自动向上让位的行为；实际显示分辨率改变时仍沿用原有重新居中逻辑。

在“更多”按钮下方原有空白处绘制与顶部一致的短横拖拽点，保持原按钮布局和工具栏高度。上下拖拽点均沿用侧栏背景的鼠标/触摸拖动路径，保留松手结束及排除右键、滚轮按键的规则。

输入区域由“原点移回屏幕内、保留原宽高”改为裁切矩形的两端，只保留与屏幕相交的部分，并跳过空区域。这样工具栏部分越界时，屏幕外区域不会被平移回桌面形成透明点击遮挡；动画刷新范围也随未受限的实际位置计算。

`cargo check --tests --examples --locked` 通过。更新旧的自动让位断言，补充上下拖拽点、左右两侧、鼠标/触摸、越界后的输入区域、松手停止及缩放/收起不跳回的回归源；仅编译，没有执行用例或操作虚拟机桌面。

生产绘制函数生成的离线预览已检查：正常工具栏上下均有短横拖拽点，靠近底部展开时保持上端位置、屏幕外部分裁切。宿主预览目录为 `C:/Users/shi01/.codex/visualizations/2026/09/30/01a0f289-5425-70b0-ae51-d22ebeb00167/classroom-drag-20261024/`，其中 `classroom-drawer.png` 展示两端拖拽点，`classroom-drawer-bottom.png` 展示底部展开分帧。静态图不能证明真实拖拽、点击穿透或触屏交互已通过，这些操作仍由用户验收。

发布构建通过，VM 已从 `.21` 覆盖安装 `3.0-Electro-testing+custom20261024.22`，dpkg 状态正常，程序和资源校验一致。DEB SHA256：`cf7c5236f61ad52eb1ce4841e5b7e4854be4980a2b631182dd39c898fa00ef9c`；主程序 SHA256：`54a78a0cc9fbc422443f35294edeca3c4ce8cd9cce0419d6157d64ebb292bc26`。WPS 加载项、图标和 GNOME 扩展校验未变，无需注销。没有结束正在运行的 Sidera（安装记录 PID 27264），用户退出重开后加载新版。

WSL 构建目录保存 `drag22-check.log`、`drag22-artwork.log`、`drag22-build.log`、`drag22-package.log`；VM 安装日志为 `/var/tmp/sidera-drag22-install.log`。回退目录保存 `.21` 包与 `sidera-before-drag22`。本地包仅用于当前 Ubuntu 26.04.1 amd64 VM，最低 libc 2.43；仓库通用打包版本和最低 libc 未改。未提交或 push。

## 后续：设置支持单项恢复默认（`.23`）

用户要求大触点阈值等设置可以一键恢复。四条数值设置右侧新增“默认”按钮，单独恢复对应项，沿用现有设置应用、保存和多屏同步路径；开关、橡皮触发方式及画布内容不受其他数值项重置影响。

| 参数 | 恢复值 |
| --- | --- |
| 工具栏大小 | 100% |
| 工具栏不透明度 | 80/255，界面显示 31% |
| 大触点阈值 | 64 |
| 大触点橡皮倍率 | 1.2 倍 |

初始值与重置值改为共用常量，保证今后调整默认值时保持一致。阈值直接恢复为初始的 64，不会因滑条 10 的步进变成 60。原设置窗口大小与行位置保留，每行右侧分为滑条、当前数值和默认按钮；滑条命中区域不再覆盖数值和按钮，避免按默认时误触拖动。默认按钮沿用松手触发及短暂按下反馈，不增加悬浮动画或确认弹窗。

`cargo check --tests --examples --locked` 通过。已有设置命中回归源扩展到手背/多指两种模式，并核对按钮不被滑条区域截获；仅编译，未执行用例或操作虚拟机桌面。实际单项恢复、触摸点击和退出重开后的保存效果由用户验收。

生产绘制函数的静态预览已检查手背与多指两种排版，数值、滑条和默认按钮分列且无重叠。预览保存在宿主 `C:/Users/shi01/.codex/visualizations/2026/09/30/01a0f289-5425-70b0-ae51-d22ebeb00167/classroom-defaults-20261024/`，分别为 `classroom-settings.png` 与 `classroom-settings-fingers.png`；这些是离线外观图，不代表已做实机操作测试。

发布构建通过，VM 已从 `.22` 覆盖安装 `3.0-Electro-testing+custom20261024.23`，dpkg 状态正常，程序和资源校验一致。DEB SHA256：`350d5763cf5facb2ae0e0429a0ecffb94fc237cb49ebca33d28c21f5728a7f53`；主程序 SHA256：`eb789be237441de90e688e7e5824e6e3af713397089263eb766ed8b3ee2d7986`。WPS 加载项、图标和 GNOME 扩展校验未变，无需注销。没有结束运行中的 Sidera（安装记录 PID 27264），由用户退出重开加载新程序。

WSL 构建目录保存 `defaults23-check.log`、`defaults23-artwork.log`、`defaults23-build.log`、`defaults23-package.log`；VM 安装日志为 `/var/tmp/sidera-defaults23-install.log`。回退目录保存 `.22` 包及 `sidera-before-defaults23`。本地包仅用于当前 Ubuntu 26.04.1 amd64 VM，最低 libc 2.43；仓库通用打包版本和最低 libc 未改。未提交或 push。

## 后续：工具栏拖动交互反馈（`.24`）

用户要求拖动时有交互提示。为被抓住的一侧增加蓝绿色边框、浅色底和两端拖拽点高亮；按住但尚未移动时显示“上下拖动”，越过原有 4 像素移动阈值后显示“拖动中”，松手立即恢复常态。两侧仍按原逻辑一起移动，高亮标识实际抓取的一侧；收起的小把手也有按住反馈。普通鼠标悬浮不会改变界面。

提示跟随抓取时的纵向偏移，靠工具栏内侧显示，并保持在屏幕内可见。它只是绘制装饰，不加入输入区域、不增加按钮或拖动限制。移动时分别重绘提示的旧、新位置；松手、取消输入、显示器移除/重配置及触摸重新接管时清除反馈，避免残留提示或高亮。拖动开始按旧边界关闭工具弹窗，避免弹窗留在原位置。

`cargo check --tests --examples --locked` 通过。已有鼠标/触摸拖动回归源补充按住、移动、错误按键释放、正确松手、越界提示和非交互区域断言，另覆盖取消后恢复常态且悬浮不重新触发；仅编译，没有执行用例或操作虚拟机桌面。实际拖动手感、显示刷新和点击穿透由用户验收。

生产绘制函数生成并检查了左右两侧的常态、按住、拖动中、松开四种静态状态；松开状态保留移动后的位置并清除提示与高亮。最终预览为宿主 `C:/Users/shi01/.codex/visualizations/2026/09/30/01a0f289-5425-70b0-ae51-d22ebeb00167/classroom-drag-feedback-20261024/classroom-drag-feedback.png`，不是虚拟机操作截图。

发布构建通过，VM 已从 `.23` 覆盖安装 `3.0-Electro-testing+custom20261024.24`，dpkg 状态正常，程序和资源校验一致。DEB SHA256：`b08edb42be2ee4a900f37b53eee18e7f5cd764192abf24f23a6b610894f317a9`；主程序 SHA256：`477f64d2b9fd5845875182f0167f0486511e0a1709cd30acbd1e498b55289695`。WPS 加载项、图标和 GNOME 扩展校验未变。没有结束运行中的 Sidera（安装记录 PID 27264），由用户退出重开生效，无需注销。

WSL 构建目录保存 `dragfeedback24-check.log`、`dragfeedback24-artwork.log`、`dragfeedback24-build.log`、`dragfeedback24-package.log`；VM 安装日志为 `/var/tmp/sidera-dragfeedback24-install.log`。回退目录保存 `.23` 包及 `sidera-before-dragfeedback24`。本地包仅用于当前 Ubuntu 26.04.1 amd64 VM，最低 libc 2.43；仓库通用打包版本和最低 libc 未改。未提交或 push。

## 后续：放映按钮状态切换动画（`.25`）

用户要求 Esc、开始放映、结束放映的切换流畅。三个状态的底色和边框采用 240 毫秒缓入缓出渐变，文字先淡出再淡入，同一帧只显示一种标题，避免单行 Esc 与两行中文重叠。保留现有暖白、绿色开始和浅红结束样式，按钮尺寸和位置不变。

视觉过渡与命令、几何动画分离：真实状态立即更新，动画不阻挡点击、不改变 WPS 命令或模拟键盘路径，不扩大输入区域，也不会触发工具栏展开。快速反向切换或中途出现第三种状态时，从当前混合比例继续；重复上报相同状态不会重启动画。左右按钮共用每帧采样值；只重绘可见按钮，工具栏自身移动时继续使用已有条带刷新。白板内保留“背景颜色”按钮，隐藏期间继续推进放映状态，不积压旧动画。

`cargo check --tests --examples --locked` 通过。新增中途切换、反向切换、重复上报不延迟结束的回归源，既有放映联动回归源补充动画时按钮仍可命中且边界不变；只编译，未执行测试用例、启动应用或操作虚拟机桌面。真实切换流畅度及动画期间点击由用户验收。

离线预览使用生产绘制代码，检查三种状态全部六个转换方向的 0、60、120、180、240 毫秒静态帧。图中 120 毫秒为文字交替的透明时点，不叠加两种标题；预览不能证明实机帧率或 WPS 操作通过。图片位于宿主 `C:/Users/shi01/.codex/visualizations/2026/09/30/01a0f289-5425-70b0-ae51-d22ebeb00167/classroom-presentation-20261024/classroom-presentation-transition.png`。

发布构建通过，VM 已从 `.24` 覆盖安装 `3.0-Electro-testing+custom20261024.25`，dpkg 状态正常，程序和资源校验一致。DEB SHA256：`193bd38f5a4db9c31b1cc7f7aed2ccd534e88f6e8ca4ff1b3d54ede10429d59e`；主程序 SHA256：`4629d29d44a1f5ae7e90b72e9cb04234a508f9d2c57e74f0559cc3da14c14e2d`。WPS 加载项、图标和 GNOME 扩展校验未变，无需重新登录或重开 WPS。运行中的 Sidera（安装记录 PID 27264）未被结束，用户退出重开后生效。

WSL 构建目录保存 `presentation25-check.log`、`presentation25-artwork.log`、`presentation25-build.log`、`presentation25-package.log`；VM 安装日志为 `/var/tmp/sidera-presentation25-install.log`。回退目录保存 `.24` 包及 `sidera-before-presentation25`。本地包仅用于当前 Ubuntu 26.04.1 amd64 VM，最低 libc 2.43；仓库通用打包版本和最低 libc 未改。未提交或 push。

## 后续：点击页码打开拨轮跳页（`.26`）

点击两侧工具栏中的页码区域，打开靠近该侧的暖白选页面板；中间突出选中页，上下显示相邻页。滚动、拖动和点击相邻数字只修改预选页，点击“跳至第 N 页”后执行一次跳转。取消、再次点击同侧页码或点击其他工具会关闭面板。鼠标悬浮不改变状态，按钮继续松手触发，工具栏位置和翻页按钮保持原样。

### 手感参数与输入归属

- 一行 44 逻辑像素，拖动死区 4 像素；按住显示选中带边框。
- 鼠标滚轮一格一页，160 毫秒缓出；触控板细小增量累计，避免零碎滚动被丢弃，不额外叠加系统惯性。
- 拖动速度使用时间平滑，限制到每秒 18 页；松手投影系数 0.16，附加惯性最多 3 页。停住超过 100 毫秒再松手时不延续旧速度。
- 松手吸附到整数页，时长约 220–316 毫秒。首末页只有最多 0.3 行的短阻尼，没有循环绕回。
- 第一个触点独占拨轮，额外触点或右键不能结束它；移出面板后继续跟随原抓取，取消输入清除拖动。X11 拖动期间不刷新重映射输入窗。
- 面板只加入自身输入区域；关闭时重绘完整旧区域。只画附近至多五行文字，不随总页数建立大量控件。

上述为实现参数与代码检查结果，并不代表实机手感已被测量。仍按用户要求不执行测试、不启动 Sidera/WPS、不注入桌面输入，实际滚动速度、触摸和停靠手感由用户验收。

### 跳转与失效处理

白板提交时先保存当前画布，再加载目标页，继续使用现有页缓存。WPS 新加载项报告跳页能力与当前文档标识，Rust 仅在状态新鲜、页数有效时允许打开拨轮。编辑视图与放映视图分别调用 WPS 的 `GotoSlide`，不依赖连续模拟翻页；批注页仍跟随 WPS 真实上报，选择过程中不改缓存。未知总页数、旧加载项、联动断开或命令失败时不暴露不可执行的跳页。

文档、编辑/放映状态或总页数变化会使旧标识失效；打开的旧拨轮关闭，队列与 WPS 执行端均检查页数和文档标识。绝对跳页队列只保留最新目标，过期命令不重放。WPS 文档名称只参与加载项内部比较，传给本地桥的是数值标识。

接口依据：[WPS 放映视图](https://open.wps.cn/documents/app-integration-dev/wps365/client/wpsoffice/jsapi/wpp/SlideShowView/obj)、[WPS 编辑视图](https://open.wps.cn/documents/app-integration-dev/wps365/client/wpsoffice/jsapi/wpp/View/obj)、[GNOME 滚动增量](https://gnome.pages.gitlab.gnome.org/mutter/clutter/method.Event.get_scroll_delta.html)。实际 WPS 版本是否提供该接口仍由运行时能力检查与用户操作验收决定。

### 编译与静态外观

`cargo check --tests --examples --locked` 通过且无本轮编译警告；加载项与 GNOME 扩展 JavaScript 语法检查通过。回归源覆盖预览不翻页、白板内容保存、触摸归属、取消、失效文档、队列替换、滚轮累计和端点停靠，只编译/检查语法，没有执行用例。

生产绘制函数的离线预览检查了首页、中间页、滚动中、白板末页及左右侧面板。目录为宿主 `C:/Users/shi01/.codex/visualizations/2026/09/30/01a0f289-5425-70b0-ae51-d22ebeb00167/classroom-page-wheel-20261024/`，包括 `classroom-page-wheel.png`、`classroom-page-wheel-left.png` 和 `classroom-page-wheel-right.png`。静态图不代表真实桌面操作已通过。

### 安装记录

VM 已从 `.25` 覆盖安装 `3.0-Electro-testing+custom20261024.26`，dpkg 状态正常。程序、WPS 加载项、系统扩展和用户扩展文件均通过 SHA256 校验，图标保持原校验值。

| 文件 | SHA256 |
| --- | --- |
| DEB | `a85ff4c2d5603d54ee4d2c9d2601615aee77ea6323fcab6d96d3df0715c9d3aa` |
| 主程序 | `03782e612bee40d82eeab252a478d266375cc494c2baed75c141c0f3ce5636b2` |
| WPS bridge.js | `4834ea8836a86dafda560837c474b18c5996064457d7e0c33989d3ed865a203f` |
| GNOME extension.js | `c3f6acf277dfc22de5974fc99b2491d69284050e9d629e724cf717c798d9ddab` |
| GNOME metadata.json | `918f5875f0fb20d0f38bac99c39ea648df50252de66a0fd580af8a42249ad0ad` |

用户扩展目录只替换 `extension.js` 和 `metadata.json`，保留已有启用设置与 schemas；旧文件已备份。只读状态显示当前 Shell 仍加载扩展 4 / ACTIVE，磁盘已是版本 5。没有重新加载扩展、结束 Sidera（PID 27264）、启动 WPS 或注销桌面。用户保存文档后注销重登，并打开 Sidera/WPS，才能完整加载新主程序、加载项与原生滚轮事件。

WSL 保存 `pagepicker26-check.log`、`pagepicker26-artwork.log`、`pagepicker26-build.log`、`pagepicker26-package.log`；VM 安装日志为 `/var/tmp/sidera-pagepicker26-install.log`。回退目录 `/var/backups/sidera-compat-20261024` 保存 `.25` 包、`sidera-before-pagepicker26` 和 `pagepicker26-before-extension.js` / `pagepicker26-before-metadata.json`。本地包面向当前 Ubuntu 26.04.1 amd64 VM，最低 libc 2.43，不能据此宣称 Ubuntu 24.04 已验收。仓库通用打包版本与最低 libc 未改，未提交或 push。

## 后续：设置页辅助署名（`.27`）

按用户要求加入 `shizhelei` 和 `Gary_6Stones`，作为次要署名。设置页保留原有 `Sidera 3.0-Electro-testing © 2026 Carl_Jin GNU GPL v3` 一行，在其下增加“定制与改进：shizhelei · Gary_6Stones”。软件名称、图标、主标题、启动页 `developed by jinyicheng` 与源码版权均保持原样。

辅助署名使用 10 逻辑像素、普通灰字及 200/255 透明度，比原版权行的 11 像素更小、更淡，不使用强调色或独立装饰。两行间距约 15 像素，底部仅增加 14 像素；通常保留原按钮和版权行的位置，短屏时允许面板略上移以保留底部空间。

`cargo check --tests --examples --locked` 通过。生产绘制函数的离线预览已检查手背与多指两种设置布局，最终图片位于宿主 `C:/Users/shi01/.codex/visualizations/2026/09/30/01a0f289-5425-70b0-ae51-d22ebeb00167/classroom-credit-20261024/` 的 `classroom-settings.png` 和 `classroom-settings-fingers.png`。没有新增或执行测试、启动应用或操作虚拟机桌面；实际显示由用户确认。

发布构建通过，VM 已从 `.26` 覆盖安装 `3.0-Electro-testing+custom20261024.27`，dpkg 状态正常。DEB SHA256：`8a48d080d39f8778b411d256c13957543660224a7857b72f990ca674ee0cf7d1`；主程序 SHA256：`b4a8bf1d87a7c7a0ece7e3110126ed712037a5ce2630ff6e16eab70e93ac7ea2`。WPS 加载项、GNOME 扩展和图标校验未变，本次没有更新用户扩展。安装时只读看到 Sidera PID 34355、35462，未结束这些进程；辅助署名在用户退出重开 Sidera 后生效。

WSL 保存 `credit27-check.log`、`credit27-artwork.log`、`credit27-build.log`、`credit27-package.log`；VM 安装日志为 `/var/tmp/sidera-credit27-install.log`。回退目录保存 `.26` 包和 `sidera-before-credit27`。本地包仍面向当前 Ubuntu 26.04.1 amd64 VM，最低 libc 2.43；仓库通用打包版本和最低 libc 未改，未提交或 push。

## 后续：屏幕内拖动与连续抓取（`.28`）

按用户新的反馈恢复屏幕限位，替代 `.22` 的允许越界行为。展开工具栏与收起小角分别按当前高度限制纵坐标，顶部可达 0、底部可达屏幕高度减工具栏高度。鼠标横向离开工具栏不结束拖动；拖动开始保留 4 逻辑像素死区，之后直接跟随事件坐标，不为移动位置增加动画。每次采样更新拖动基点，越界部分不积累，因此反向移动立即响应；释放坐标也纳入最终位置，避免遗漏最后一段移动。

布局以用户的首选停靠位置为基础计算可见位置。工具组展开、WPS 页码行出现和调大工具栏时，靠下的工具栏向上让位；缩短后可恢复原停靠意图。展开动画的绘制与输入范围覆盖整个移动区间，避免残影和中途误点。左侧与右侧维持既有同步移动，顶部与底部拖拽点、触摸归属和收起小角拖动均保留。

拖动期间仅高亮当前侧工具栏边框与上下拖拽点，取消 `.24` 的文字气泡。松开和取消恢复常态，不增加鼠标悬浮反馈。

### GNOME 鼠标抓取

扩展 5 只保留原输入 actor、依赖隐式抓取，无法保证鼠标离开窄输入区域后仍收到移动与释放。扩展 6 在输入 actor 收到鼠标左键/右键按下时创建 `Clutter.Stage.grab`，保留到物理按键全部释放；原 actor 在此期间不重建。键盘焦点和 `Main.modalCount` 不变。其他桌面的 X11/layer-shell 继续由原生协议提供隐式抓取。

正常松手、真实按键状态消失、其他 Shell 抓取撤销当前抓取、锁屏/概览、断连和扩展停用均释放。Shell 撤销回调延迟到空闲处理，避免在 Clutter 更新抓取链时销毁 actor；被打断的拖动不会在菜单关闭后自动恢复。持续按住但不移动不会被闲置超时误判为松手。释放抓取发生在发送客户端释放事件之前，以便后续模拟键盘/右键继续交给桌面。

接口依据：[Stage.grab](https://mutter.gnome.org/clutter/method.Stage.grab.html)、[Mutter 50.1 stage/grab 实现](https://github.com/GNOME/mutter/blob/50.1/clutter/clutter/clutter-stage.c)、[Mutter 46 revoked 属性](https://github.com/GNOME/mutter/blob/46.0/clutter/clutter/clutter-grab.c)。版本兼容来自源码检查，不代表 GNOME 46–50 已逐一实测。

### 验证边界

`cargo check --tests --examples --locked` 和扩展 JavaScript 语法检查通过。更新的回归源覆盖两侧上下抓取点、移出工具栏、屏幕边界、即时反向、最终释放坐标、收起小角、展开与尺寸变化。只编译源文件，未执行测试。

生产绘制函数的静态图片检查了左右侧按下/拖动/释放高亮，以及贴底展开的 0/35/70/120/220 毫秒分帧；文字气泡已消失，贴底展开全部可见。宿主预览目录：`C:/Users/shi01/.codex/visualizations/2026/09/30/01a0f289-5425-70b0-ae51-d22ebeb00167/classroom-drag-bounds-20261024/`，图片为 `classroom-drag-feedback.png`、`classroom-drawer-bottom.png`。静态图片不能验证帧率、抓取或真实触摸。

按用户要求未启动 Sidera/WPS、注入输入、结束运行进程或注销桌面。更新前只读确认 VM 安装 `.27`，GNOME 扩展 5 / ACTIVE，Sidera PID 39335。用户安装后保存内容并注销重登加载扩展 6，再验收“从上下拖拽点按住→横向移出工具栏→拖至顶/底→反向拖动→在工具栏外松手”，同时确认光标模式恢复点击穿透、收起小角不会因拖动而展开。

### 安装记录

发布构建通过，VM 已从 `.27` 覆盖安装 `3.0-Electro-testing+custom20261024.28`，dpkg 状态正常。系统与用户扩展均已写入版本 6；仅替换用户目录的 `extension.js` 和 `metadata.json`，保留其余文件与启用设置。旧主程序、`.27` 包和用户扩展原文件已备份至 `/var/backups/sidera-compat-20261024`（`sidera-before-drag28`、`drag28-before-extension.js`、`drag28-before-metadata.json`）。

| 文件 | SHA256 |
| --- | --- |
| DEB | `a980d893d62fd342d28f85877cf3a28549ad022205b95f9ed86ade1d21e36252` |
| 主程序 | `a50c920175988d4d7e90374eb947ae0d9b38a401a3489ca16c418f131258ce6d` |
| GNOME extension.js | `943e1d9362ed6f23d1fc22a62b1b5a689cb49520cb6de908c24cbd3b60c9dd22` |
| GNOME metadata.json | `d6bd232627574bc02def08b24103d182c67e974613678db432735ef7728ff8d6` |

安装后逐个校验主程序、系统/用户扩展、WPS bridge.js 与两个图标文件，全部一致；WPS 加载项和图标内容未变。只读状态仍为扩展 5 / ACTIVE，运行中的 Sidera PID 39335 未变；仅重开程序不足以启用新的 GNOME 抓取，仍需用户保存内容后注销重登。未重载 Shell、未运行桌面测试。

WSL 保存 `drag28-check.log`、`drag28-artwork.log`、`drag28-build.log`、`drag28-package.log`；VM 安装日志 `/var/tmp/sidera-drag28-install.log`。工作区修改源文件与 WSL 构建源按换行规范化后逐文件一致。本地包面向当前 Ubuntu 26.04.1 amd64 VM，最低 libc 2.43；仓库通用打包版本与最低 libc 未改，未提交或 push。

## 后续：课堂启动页与工具栏入场（`.29`）

### 视觉构思与署名

沿用课堂浅色配色：居中暖白卡片、薄荷浅色品牌区、原软件图标、深灰标题及低饱和蓝绿进度。设计尺寸为 560×448 逻辑像素，小屏按比例缩小并留出边缘；按输出缩放绘制。品牌区展示 Sidera、3.0 与“屏幕批注 · 白板书写 · 课件联动”。四行准备状态对齐排列，已准备使用勾号，等待使用时钟，异常使用琥珀色提示。底部保留原作者 `developed by jinyicheng`；应用户补充要求，其下增加 11 像素、190/255 透明度的“定制与改进：shizhelei · Gary_6Stones”，主作者为 13 像素，保持主次分明。

### 真实状态与共用流程

原闪屏仅在 X11 入口调用，GNOME 实现为空，且进度条由固定等待时间推算。现改为三种后端共用的 `Launch` 状态与生产 UI 绘制，不额外创建闪屏窗口、不用休眠阻塞事件循环。实际桌面连接、字体/图标/设置读取和画布配置完成后呈现卡片，再接收首次 WPS 服务初始化尝试的结果。

- 桌面连接：当前后端名称与逻辑屏幕尺寸。
- 界面资源：根据已加载字体与图标显示完整或替代资源状态。
- 绘图交互：画笔/白板已准备；快捷键注册成功才显示快捷键已准备。
- WPS：准备中、按设置关闭、服务准备但等待 WPS、已接入、服务暂不可用分别显示；可选联动未接入或失败不阻止批注启动。

四段进度表示初始化步骤已处理，时间只控制阅读和动画，不冒充加载百分比；准备结果未到达时，即使经过很长时间也不会自动宣称就绪。卡片至少展示 1100 毫秒，取得准备结果后至少保留 250 毫秒；随后 220 毫秒淡出。卡片内容缓存成位图，状态或渲染尺寸变化才重绘文字，淡出期间只合成透明度。

### 工具栏中央展开与输入

卡片开始淡出 140 毫秒后，左右工具栏各自从最终位置的纵向中点向两端展开，时长 380 毫秒、三次缓出，和卡片尾部淡出重叠 80 毫秒。背景与上下拖拽点随高度展开；按钮按最终尺寸绘制，完整进入可见空间才逐渐显现，避免纵向压缩图标或切断文字。结束时保持原位置、尺寸、绘图组收起状态和屏幕限位。

启动阶段只拦截卡片区域，以及工具栏开始出现后的两侧窄条，其余桌面继续穿透。展开期间按钮不响应；完成后移除卡片输入区域并恢复正常工具栏输入。触摸开始于介绍阶段时记录为忽略，保持按住跨过动画结束也不会变成画笔笔迹。各帧不重映射整个显示层；工具栏动画输入区不逐帧改变，减少 XWayland 刷新开销。

只对初始配置的输出启动介绍，重复唤醒/重绘、改变分辨率或之后插入投影仪不重播；移除输出时结束其介绍。`SIDERA_NO_SPLASH` 同时跳过卡片和工具栏入场，原有验证环境入口继续可用。GNOME 扩展和 WPS 加载项不需要为此功能改动。

### 验证边界

`cargo check --tests --examples --locked` 通过。回归源覆盖真实准备结果前不结束、可选 WPS 状态、重复心跳不重启计时、淡出/展开单调性、完整展开后才能点击、卡片输入区回收、屏幕内对称展开，以及按住触摸跨越启动的归属；按用户要求只编译，未执行用例。

使用生产绘制函数生成并检查四种启动状态与左右工具栏 0/40/100/180/280/380 毫秒入场分帧。首版预览后调整状态字号、图标衬底与主次署名。宿主目录：`C:/Users/shi01/.codex/visualizations/2026/09/30/01a0f289-5425-70b0-ae51-d22ebeb00167/classroom-startup-20261024/`，图片为 `classroom-startup.png`、`classroom-startup-states.png`、`classroom-startup-reveal.png`。静态预览不能证明真实帧率、桌面启动或输入体验已通过，操作验收仍由用户进行。

### 安装记录

发布构建通过，VM 已从 `.28` 覆盖安装 `3.0-Electro-testing+custom20261024.29`，dpkg 状态正常。DEB SHA256：`fb7d1665d3205ef3a080803888320e2d8dd35c96fe561e9783c1474e011eadba`；主程序 SHA256：`a674498045021422f30d29b0fbdfba831d85ba857e363447be84eac06483681f`。主程序、WPS 加载项、系统/用户 GNOME 扩展与图标逐项校验一致；扩展、加载项和图标内容未变。本轮无需重新登录或更新用户扩展，只读状态已为扩展 6 / ACTIVE。安装前后未检测到 Sidera 进程，没有启动应用、注入输入、结束进程或注销桌面。

工作区与 WSL 构建源按换行规范化后逐文件一致。WSL 保存 `startup29-check.log`、`startup29-artwork.log`、`startup29-build.log`、`startup29-package.log`；VM 安装日志 `/var/tmp/sidera-startup29-install.log`。回退目录 `/var/backups/sidera-compat-20261024` 保存 `.28` 包和 `sidera-before-startup29`。本地包针对当前 Ubuntu 26.04.1 amd64 VM，最低 libc 2.43；仓库通用打包版本与最低 libc 未改，仍在 `custom`，未提交或 push。

## 后续：返回有笔迹页自动展开绘图工具组（本地 `.30`）

用户澄清问题为“空白页 → 有笔迹页，应展开”。旧逻辑只排队自动收起检查，而且工具组已经收起时不再排队；原有回归源也把翻回笔迹页仍不展开作为预期，因而缺少反向恢复。

现区分“切换光标时只检查能否收起”与“恢复页面后根据内容更新工具组”。`load_page` 对透明批注页始终排队页面更新，随后检查实际画布 alpha，不把缓存存在或保守的 `page_has_ink` 标记当作真实笔迹。空白页在光标模式下收回，有笔迹页展开，保持当前光标/画笔/橡皮模式；不在闲置帧反复扫描或重启动画。

GNOME / layer-shell 在所有输出完成页面同步后统一判断，任何屏幕有笔迹就展开共享工具组，并更新共享控制状态。白板、仍在绘图的输入以及手动贴边收起的整条工具栏不参与自动调整；用户显式选择工具或收起会取消先前排队的自动调整。沿用 220 毫秒动画，并同步更新绘制与输入区域。

更新的回归源覆盖空白与有笔迹来回翻页、擦空但缓存仍存在、WPS 等待真实页号、投影屏独有笔迹、当前模式保持及手动收起不被翻页打开。`cargo check --tests --examples --locked` 通过；仅编译这些源文件，未执行测试用例或桌面操作。发布构建通过，修改的 5 份 Rust 源文件与 WSL 构建副本按换行规范化后相同。

VM 从 `3.0-Electro-testing+custom20261024.29` 覆盖安装到 `.30`，安装退出 0，包状态正常。DEB SHA256：`67eb5d44ef984947ec885cf7d7cedd717ab80a591472cd141f7443c961138b06`；主程序 SHA256：`8e6bb002d19610b3fcdeb97ccb19527c6a71d14bb5a0dcd6220edc08fe137eb2`。主程序、WPS 加载项及系统/用户 GNOME 扩展逐项校验一致；扩展仍为 6 / ACTIVE，无需重新登录。

安装前后 Sidera PID 4565 保持运行，未启动或结束应用、注入输入或操作桌面。使用者保存需要的批注后退出重开 Sidera，再验证“有笔迹页 → 空白页收回 → 返回有笔迹页展开”；当前旧进程尚未加载新主程序。

WSL 日志为 `tools30-check.log`、`tools30-build.log`、`tools30-package.log`；VM 安装日志为 `/var/tmp/sidera-tools30-install.log`。回退目录 `/var/backups/sidera-compat-20261024` 保存 `.29` 包与 `sidera-before-tools30`。包仍针对 Ubuntu 26.04 amd64 / libc 2.43；此修复保留在本地 `custom`，没有更新已经公开的 `.29` release。
