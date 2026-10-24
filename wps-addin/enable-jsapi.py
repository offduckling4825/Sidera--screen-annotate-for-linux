#!/usr/bin/env python3
"""Enable the documented WPS add-in switch, preserving and backing up oem.ini.

Run only when the installed WPS edition includes JSAPI but does not load add-ins:
  sudo python3 /usr/share/sidera/wps-addin/enable-jsapi.py
Restart WPS afterwards. This does not enable the web debugger or change servers.
"""
from pathlib import Path
import argparse
import os
import re
import stat
import tempfile


def enable(path):
    if path.is_symlink() or not path.is_file():
        raise ValueError(f"不是普通 WPS 配置文件：{path}")
    original = path.read_bytes()
    text = original.decode("utf-8-sig")
    newline = "\r\n" if "\r\n" in text else "\n"
    # The published API uses [Support]. Keep the vendor's other sections and
    # values verbatim, including its separate lower-case [support] section.
    section = re.search(r"(?m)^\[Support\][ \t]*\r?$", text)
    setting = "JsApiPlugin=true" + newline
    if section:
        following = re.search(r"(?m)^\[", text[section.end():])
        end = section.end() + following.start() if following else len(text)
        body = text[section.end():end]
        key = re.compile(r"(?m)^[ \t]*JsApiPlugin[ \t]*=.*(?:\n|$)")
        if len(key.findall(body)) > 1:
            raise ValueError("配置包含重复 JsApiPlugin，请先检查；未修改原文件")
        if key.search(body):
            body = key.sub(lambda _: setting, body)
        else:
            body = newline + setting + body.lstrip("\r\n")
        updated = text[:section.end()] + body + text[end:]
    else:
        updated = text.rstrip("\r\n") + newline + newline + "[Support]" + newline + setting
    contents = updated.encode("utf-8")
    if original.startswith(b"\xef\xbb\xbf"):
        contents = b"\xef\xbb\xbf" + contents
    if contents == original:
        print("WPS 加载项开关已开启")
        return
    metadata = path.stat()
    with tempfile.NamedTemporaryFile(dir=path.parent, prefix="oem.ini.sidera-backup-", delete=False) as backup:
        backup.write(original)
        backup.flush()
        os.fsync(backup.fileno())
        backup_path = Path(backup.name)
    pending = None
    try:
        with tempfile.NamedTemporaryFile(dir=path.parent, prefix=".sidera-oem-", delete=False) as output:
            pending = Path(output.name)
            output.write(contents)
            output.flush()
            os.fchmod(output.fileno(), stat.S_IMODE(metadata.st_mode))
            os.fchown(output.fileno(), metadata.st_uid, metadata.st_gid)
            os.fsync(output.fileno())
        os.replace(pending, path)
    finally:
        if pending and pending.exists():
            pending.unlink()
    print(f"已开启 WPS 加载项；原配置备份：{backup_path}")
    print("请保存文档，退出 WPS 的所有窗口后重新打开。")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--oem", type=Path, default=Path("/opt/kingsoft/wps-office/office6/cfgs/oem.ini"))
    args = parser.parse_args()
    try:
        enable(args.oem)
    except (OSError, UnicodeError, ValueError) as error:
        parser.exit(1, f"WPS 配置未更新：{error}\n")
