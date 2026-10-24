# 许可证说明

## 主项目：GNU GPL v3

Sidera 来自 [offduckling4825/Sidera--screen-annotate-for-linux](https://github.com/offduckling4825/Sidera--screen-annotate-for-linux)，原始版权为 **Copyright (C) 2026 Carl_Jin**。本仓库保留完整 [LICENSE](LICENSE)，不把原项目或其定制版本改称 MIT 项目。

原始 LICENSE 的项目声明包含“GPL v3 或以后版本”，Rust 包元数据使用 `GPL-3.0-only`；本次发布按 GNU GPL v3 条款提供，原作者的授权声明保持原样。贡献者与改动归属见 [NOTICE.md](NOTICE.md)。

分发此程序或修改版本时，请保留版权与许可声明，并依照 GPL 提供相应源代码。各 release 对应的 Git tag、源代码下载及构建说明与二进制一起发布。具体权利和义务以 [LICENSE](LICENSE) 与 [GNU GPL v3 官方文本](https://www.gnu.org/licenses/gpl-3.0.html) 为准。

## MIT 与其他第三方许可

“依赖采用 MIT”与“整个项目采用 MIT”不是同一件事。Rust 依赖、Wayland 协议、第三方修补材料可能各自采用 MIT、Apache、BSD 或其他许可，它们的原版权与授权仍需保留。

- Rust 依赖版本以 [Cargo.lock](rust/Cargo.lock) 为准，授权以对应 crate 的许可文件为准。
- Wayland 协议来源及许可保留在 [protocols/](protocols/) 的文件头。
- Xwayland 回补保留原补丁作者、来源和独立适用范围，见[回补说明](compat/xwayland-24.1.10/README.md)。
- 本次 release 附 `THIRD_PARTY_NOTICES.md`，列出所用依赖的许可和原始许可文本；它不替换主项目的 GPL。
- README 预览来自本仓库的绘制代码；图标沿用原标志并调整配色，不宣称重新原创。

如果引入新的素材或第三方代码，请同时记录来源、版本及许可，不要仅添加一个 MIT 标记覆盖其他作者的授权。
