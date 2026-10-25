#!/usr/bin/env python3
"""Assemble a fresh USB folder after tests; never deletes/overwrites a destination."""
import argparse
import hashlib
import json
import re
from pathlib import Path
import shutil
import subprocess
import tarfile

TAG = "v3.0-Electro-testing-custom20261025.35"
VERSION = "3.0-Electro-testing+custom20261025.35"
KIT_ID = "sidera-school-usb-20261025-v2"


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    parser.add_argument("--source", required=True, type=Path)
    parser.add_argument("--source-commit", required=True, help="Exact full commit of the supplied release source")
    parser.add_argument("--assets", required=True, type=Path)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--records", required=True, type=Path)
    parser.add_argument("--scripts", type=Path, default=Path(__file__).parent)
    args = parser.parse_args()
    COMMIT = args.source_commit
    if not re.fullmatch(r"[0-9a-f]{40}", COMMIT):
        raise SystemExit("A full source commit is required")
    source_info = (args.source / "SOURCE-INFO.md").read_text()
    if COMMIT not in source_info or TAG not in source_info:
        raise SystemExit("Release source provenance does not match")
    if args.destination.exists():
        raise SystemExit("Destination already exists; preserve it and choose a fresh folder")
    for filename, marker in (("installer-tests.txt", "\nOK\n"), ("arm64-startup-tests.txt", "\nOK\n"), ("arm64-rust-tests.txt", "test result: ok.")):
        text = (args.records / filename).read_text()
        if marker not in text or "FAILED" in text:
            raise SystemExit("Required test has not passed: " + filename)
    rust_result = re.search(r"test result: ok\. (\d+) passed; 0 failed;", (args.records / "arm64-rust-tests.txt").read_text())
    if not rust_result or int(rust_result[1]) < 107:
        raise SystemExit("Missing settings regression tests in ARM64 results")
    root = args.destination
    kit = root / "Sidera_Installer"
    kit.mkdir(parents=True)
    for directory in ("packages", "backup-packages", "records", "fonts", "licenses", "source", "logs", "backups"):
        (kit / directory).mkdir()
    for name in ("sidera_usb.py", "usb_autostart.py", "start.sh", "README.md"):
        shutil.copy2(args.scripts / name, kit / name)
    (kit / "kit-id.txt").write_text(KIT_ID + "\n", encoding="utf-8")
    for name in ("LICENSE", "NOTICE.md", "LICENSES.md"):
        shutil.copy2(args.source / name, kit / "licenses" / name)
    shutil.copy2(args.assets / "THIRD_PARTY_NOTICES.md", kit / "licenses")
    zig_roots = list(args.records.glob("venv/lib/python*/site-packages/ziglang"))
    if len(zig_roots) != 1:
        raise SystemExit("Cannot locate Zig runtime license records")
    shutil.copy2(zig_roots[0] / "LICENSE", kit / "licenses/ZIG-LICENSE.txt")
    shutil.copy2(zig_roots[0] / "lib/libunwind/LICENSE.TXT", kit / "licenses/LLVM-LIBUNWIND-LICENSE.txt")
    shutil.copy2("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc", kit / "fonts")
    shutil.copy2("/usr/share/doc/fonts-noto-cjk/copyright", kit / "fonts/FONT-LICENSE.txt")
    shutil.copy2(args.assets / "Sidera-custom20261025.35-source-with-dependencies.tar.gz", kit / "source")
    packages = []
    portable = kit / "packages/sidera-custom35-arm64-glibc231.tar.gz"
    with tarfile.open(portable, "w:gz", dereference=True) as archive:
        archive.add(args.binary, arcname="bin/sidera")
        archive.add(args.source / "sidera.png", arcname="icon.png")
        archive.add(args.source / "wps-addin", arcname="share/wps-addin")
    for relative in ("packages/" + portable.name, "backup-packages/" + portable.name):
        target = kit / relative
        if target != portable:
            shutil.copy2(portable, target)
        packages.append({"file": relative, "sha256": sha(target), "kind": "portable", "architecture": "arm64",
                         "version": VERSION, "variant": "arm64-glibc231", "source_commit": COMMIT})
    published = {}
    for line in (args.assets / "SHA256SUMS").read_text().splitlines():
        checksum, name = line.split(maxsplit=1)
        published[name.lstrip("*")] = checksum
    for architecture in ("arm64", "amd64"):
        name = "sidera_" + VERSION + "_" + architecture + ".deb"
        if sha(args.assets / name) != published[name]:
            raise SystemExit("Published DEB checksum mismatch")
        shutil.copy2(args.assets / name, kit / "packages")
        packages.append({"file": "packages/" + name, "sha256": published[name], "kind": "deb", "architecture": architecture,
                         "version": VERSION, "variant": "published-" + architecture, "source_commit": COMMIT})
    (kit / "catalog.json").write_text(json.dumps({"kit": KIT_ID, "packages": packages}, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    for name in ("installer-tests.txt", "arm64-startup-tests.txt", "arm64-rust-tests.txt", "arm64-test-build.txt", "elf-needed.txt", "elf-versions.txt", "focal-runtime-provenance.json"):
        shutil.copy2(args.records / name, kit / "records")
    for name in ("compile-arm64.sh", "verify-arm64.sh", "verify-rust-arm64.sh", "fetch-focal.py"):
        source = args.records / name
        if source.is_file():
            shutil.copy2(source, kit / "records")
    for name in ("test_usb_kit.py", "sidera_usb.py", "usb_autostart.py"):
        shutil.copy2(args.scripts / name, kit / "records")
    shutil.copy2(Path(__file__), kit / "records/build_bundle.py")
    test_count = re.search(r"Ran (\d+) tests", (args.records / "installer-tests.txt").read_text())[1]
    info = f"""Sidera 学校 U 盘构建记录
软件源码标签：{TAG}
源码提交：{COMMIT}
软件版本：{VERSION}（包含深色模式，白板底色保留原样）
ARM64 兼容程序 SHA256：{sha(args.binary)}
目标：aarch64-unknown-linux-gnu.2.31；实际引用最高 GLIBC_2.30。
工具：Rust 1.93.1 / cargo-zigbuild 0.20.1 / Zig 0.14.1。
依赖：源码归档 vendor，Cargo.lock 未改，离线 locked 构建。
测试：安装器 {test_count} 项（含离线和自启动接管/恢复）；ARM64 QEMU + Ubuntu glibc 2.31 启动 16 项；普通 Rust {rust_result[1]} 项。
协议专用 13 项按普通 Rust 套件原定义忽略；并非全部桌面测试通过。
学校机器仍待实际验收。本包未在真实学校桌面或 WPS 上自动操作。
不随包安装 glibc，不更改系统图形库或原版包。
完整源代码及依赖在 source，原项目署名及许可在 licenses。
"""
    (kit / "records/BUILDINFO.txt").write_text(info, encoding="utf-8")
    for name, action in (("start.sh", ""), ("install.sh", "install"), ("install-offline.sh", "offline"),
                         ("launch-new.sh", "launch"), ("launch-original.sh", "original"), ("restore-original.sh", "restore"),
                         ("diagnose.sh", "diagnose")):
        text = '#!/bin/bash\nset -euo pipefail\nHERE="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"\nbash "$HERE/Sidera_Installer/start.sh" ' + action + ' "$@"\n'
        (root / name).write_text(text, encoding="utf-8")
    instructions = """Sidera 学校安装 U 盘

用于银河麒麟 V10 SP1 / ARM64 / X11 / glibc 2.31。
最新 3.0 custom.35（含深色模式）的旧系统兼容构建已经放在盘内。

最快操作：
1. 插到学校电脑，在 U 盘空白处右键“在终端中打开”。
2. 输入：bash ./start.sh
3. 输入 1 完全离线安装；不会查询网络，不要加 sudo。
4. 从应用菜单打开“Sidera 课堂新版”。先保存批注，正常退出旧版。

新版异常退出会在 X11 下退回原版；若功能不正常，退出新版，
打开“Sidera 原版（保留）”，也可以运行：bash ./restore-original.sh
原来的 Sidera 图标、系统安装包和配置保留。

自启动不会同时启用两版：默认保留旧版自启，新版手动启动。
需要新版随登录启动时选 7，工具先备份并停用旧版入口，再启用唯一的新入口。
新版应用内的自启动开关也同步到这个入口；接管后关闭，旧版也继续停用。
选 9 或恢复原版时，还原原先的自启设置；选 8 可关闭已接管的自启。

要回传检查结果：插回此 U 盘，运行 bash ./diagnose.sh。
报告位于 Sidera_Installer/logs；双击其中 report.html 即可阅读。
原版备份在电脑的 backup-original 与 U 盘的 backups 内，含配置，请勿公开上传。

完整 WPS 联动为可选项：先关闭 WPS，再在菜单选 6。
不启用也可按应用能力使用模拟翻页；WPS 软件本体不会被改装。

安装后不需要一直插着 U 盘。不要双击 packages 里的 DEB 强行覆盖。
不要升级 glibc 或替换系统图形库。真实触摸、穿透和课件仍需现场验收。
这些脚本在 Linux 运行；Windows 上可打开 help.html 阅读。
"""
    (root / "README.txt").write_text(instructions, encoding="utf-8")
    page = '''<!doctype html><html lang="zh-CN"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Sidera · 学校安装 U 盘</title><style>
*{box-sizing:border-box}body{margin:0;background:#f3f6f4;color:#243a35;font:17px/1.8 system-ui,"Microsoft YaHei",sans-serif}main{max-width:960px;margin:auto;padding:50px 28px}h1{font-size:38px;line-height:1.3;margin:12px 0}h2{font-size:22px}p{color:#50675f}.tag{color:#267b68;font-weight:700;font-size:14px;letter-spacing:2px}.cards{display:grid;grid-template-columns:repeat(auto-fit,minmax(220px,1fr));gap:16px;margin:30px 0}.card,section{border:1px solid #d8e4dc;background:white;border-radius:18px;padding:24px}section{margin:20px 0}b{color:#244d40}code{display:block;background:#eaf3ee;border-radius:10px;padding:16px 20px;font-size:20px;color:#17543e;overflow-wrap:anywhere}li{margin:14px 0}a{color:#267b68}small{color:#6b7c75}.notice{border-left:4px solid #b58b42;padding-left:18px}footer{margin:30px 0;font-size:13px;color:#718277}</style><main>
<div class="tag">SIDERA / CLASSROOM</div><h1>安装新版，保留原版。</h1><p>为银河麒麟 V10 SP1 · ARM64 · X11 准备<br>3.0 custom.35 · 包含深色模式 · 白板底色保持原样</p>
<div class="cards"><div class="card"><b>完全离线</b><p>只使用盘内版本最新且兼容的安装包。无需网络，校验失败尝试备用副本。</p></div><div class="card"><b>原版随时可用</b><p>并存安装，独立配置。保留原入口，异常退出可退回原版。</p></div><div class="card"><b>过程有记录</b><p>本机与 U 盘双份报告，包含检查原因、安装结果和运行日志。</p></div></div>
<section><h2>第一次安装</h2><ol><li>在学校电脑插入 U 盘，空白处右键 → <b>在终端中打开</b>。</li><li>输入下面一行，按回车：<code>bash ./start.sh</code></li><li>输入 <b>1</b> 完全离线安装，不会查询网络。<b>不要使用 sudo。</b></li><li>保存批注、正常退出旧版，从应用菜单打开 <b>Sidera 课堂新版</b>。</li></ol><p>若找不到“在终端中打开”，打开终端，输入 bash 和空格，把 U 盘的“start.sh”拖入终端，再回车。</p></section>
<section><h2>只启动一个版本</h2><p>默认保留旧版自启，新版手动运行。要让新版自动启动，菜单选 <b>7</b>：先备份、停用旧版入口，再启用唯一的新入口。</p><p>新版应用内的自启动开关会同步生效。接管后关闭，两版都不会自启；选 <b>9</b> 或恢复原版时，还原原来的设置。菜单选 <b>8</b> 可关闭已接管的自启。</p></section><section><h2>需要恢复时</h2><p>新版正常退出后，打开 <b>Sidera 原版（保留）</b>。也可从“安装与恢复”选择切回原版。旧版图标一直保留。</p><code>bash ./restore-original.sh</code><p>若新版异常退出，工具会在原生 X11 下尝试原版；若只是功能异常或卡住，请正常退出后手动切回，不会强制结束你的程序。</p></section>
<section><h2>日志与 WPS</h2><p>回传报告前插回 U 盘并运行：</p><code>bash ./diagnose.sh</code><p>查看 <a href="Sidera_Installer/logs/">logs 文件夹</a>，每次记录都有“report.html”。原版备份含配置，请勿公开上传。</p><p>需要完整 WPS 联动：保存并关闭 WPS，运行开始菜单选 <b>6</b>。安装器不替换 WPS 本体；回退会恢复本次登记。</p></section>
<p class="notice">请勿手动安装 packages 内的 DEB，也不要升级系统 glibc。已完成隔离代码与启动检查；学校硬件上的触摸、穿透和课件功能需要现场验收。安装后可拔走 U 盘。</p><footer>Sidera 原作与开源许可见盘内 licenses。本套工具由定制分支提供，非上游官方发行版。</footer></main></html>'''
    (root / "help.html").write_text(page, encoding="utf-8")
    # Only control files gate startup: a corrupt optional package must not prevent fallback.
    names = ("sidera_usb.py", "usb_autostart.py", "start.sh", "catalog.json", "kit-id.txt")
    (kit / "CONTROL-SHA256SUMS.txt").write_text("".join(sha(kit / n) + "  " + n + "\n" for n in names), encoding="utf-8")
    all_files = sorted(p for p in root.rglob("*") if p.is_file())
    (root / "SHA256SUMS.txt").write_text("".join(sha(p) + "  " + str(p.relative_to(root)) + "\n" for p in all_files), encoding="utf-8")
    print(json.dumps({"folder": str(root), "files": len(all_files) + 1, "bytes": sum(p.stat().st_size for p in root.rglob("*") if p.is_file()), "portable_sha256": sha(portable)}))


if __name__ == "__main__":
    main()
