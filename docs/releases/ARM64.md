# 为已有 Release 补充 ARM64 安装包

[`Build ARM64 release asset`](../../.github/workflows/release-arm64.yml) 是手动构建入口，使用 GitHub 原生 ARM64 构建机和 Rust 1.93.1。可选择 `ubuntu-22.04-arm`（默认）或 `ubuntu-24.04-arm`，实际依赖随构建系统确定。支持的 runner 标签见 [GitHub 官方文档](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)。无需在 Windows、VMware 或学校电脑上安装交叉编译工具。

## 来源与版本

输入已发布的 `v3.0-Electro-testing-customYYYYMMDD.N` tag。应用、WPS 加载项、GNOME 扩展和 Cargo.lock 全部取自该 tag，包版本对应 `3.0-Electro-testing+customYYYYMMDD.N`。

构建工具来自触发工作流的分支提交。旧 tag 中固定写死依赖的打包辅助脚本会替换为当前经过检查的脚本，使用实际 ARM64 ELF 计算依赖；不会把分支上的新应用功能混入旧 Release。`BUILDINFO-arm64.md` 分别记录应用提交与打包工具提交，`BUILD-SCRIPTS-arm64.tar.gz` 保存实际使用的脚本。

## 构建与核对

1. 在 Actions 中手动运行该工作流，输入已有 Release tag 和构建系统。
2. 工作流先检查构建机确为 AArch64 / arm64，再编译锁定依赖的应用；测试与示例也执行编译检查。
3. 打包后解包核对主程序字节、版本、架构和动态依赖，通过 APT 模拟依赖解析。
4. 在原生 ARM64 上对包内主程序运行隔离启动套件；使用私有 HOME、网络和虚拟显示，不连接真实桌面或 WPS。
5. 下载成功任务的 artifact，核对 `SHA256SUMS-arm64`，再把 DEB、构建记录、脚本、测试报告和校验文件补到**同一 tag** 的 Release。

工作流只有读取仓库的权限，成功后生成 artifact，**不会自动发布或覆盖 Release 资产**。正式上传后应重新下载 DEB 并核对哈希，发布说明需区分原 amd64 与新增 ARM64 的实际系统库要求。不要移动旧 tag 或把最新分支二进制放进旧版本。

## 下载后的验证

从同一 Release 下载 ARM64 DEB 和 `SHA256SUMS-arm64`，在下载目录执行：

```sh
dpkg --print-architecture
getconf GNU_LIBC_VERSION
sha256sum --check SHA256SUMS-arm64 --ignore-missing
dpkg-deb -f ./sidera_*_arm64.deb Architecture Version Depends
```

只保留本次要安装的一份匹配文件，再使用 `sudo apt install ./实际包名_arm64.deb`。不要在 amd64 机器上强装，不要跳过依赖检查或替换系统 libc。

ARM64 原生编译和隔离启动通过，不等于所有 ARM 设备、麒麟版本、图形桌面、触摸屏或 WPS 版本均通过验收。GNOME 原生扩展仍受其元数据声明的 Shell 版本限制；安装包不包含 WPS 或系统图形修复。具体已发布文件和验证范围见对应 Release 的 `BUILDINFO-arm64.md`。
