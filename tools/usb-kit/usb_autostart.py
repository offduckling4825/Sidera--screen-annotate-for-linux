"""Reversible, per-user XDG autostart ownership. Never writes system entries."""
import configparser
import os
from pathlib import Path
import re

MANAGED = "sidera-usb-managed.desktop"


def quote(value):
    return '"' + str(value).replace('\\', '\\\\').replace('"', '\\"').replace('`', '\\`').replace('$', '\\$').replace('%', '%%') + '"'


def desktop_fields(data):
    parser = configparser.ConfigParser(interpolation=None, strict=False)
    parser.read_string(data.decode("utf-8-sig"))
    return dict(parser["Desktop Entry"]) if parser.has_section("Desktop Entry") else {}


def is_sidera(name, data):
    fields = desktop_fields(data)
    return name.lower().startswith("sidera") or bool(re.search(
        r"(?:^|[/\s\"'])sidera(?:$|[\s\"'])", fields.get("exec", "")))


class Autostart:
    def __init__(self, base, config, write, save, read, log, system_dirs=None):
        self.base = Path(base)
        self.directory = Path(config) / "autostart"
        self.write, self.save, self.read, self.log = write, save, read, log
        self.journal = self.base / "autostart-journal.json"
        self.system_dirs = [Path(p) / "autostart" for p in (
            system_dirs if system_dirs is not None else os.environ.get("XDG_CONFIG_DIRS", "/etc/xdg").split(":")) if Path(p).is_absolute()]
        self.recovered = False

    def state(self):
        return self.read(self.journal)

    def enabled(self):
        state = self.state()
        return state.get("phase") == "active" and state.get("enabled", False)

    def managed_bytes(self, enabled=True):
        return ("[Desktop Entry]\nType=Application\nName=Sidera\n"
                "Exec=/usr/bin/python3 {} autostart\nTerminal=false\n"
                "Hidden={}\nX-Sidera-USB-Managed=true\n").format(
                    quote(self.base / "manager.py"), "false" if enabled else "true").encode()

    def current(self, name):
        if Path(name).name != name or not name.endswith(".desktop"):
            raise RuntimeError("自启动记录的文件名无效")
        path = self.directory / name
        if path.is_symlink():
            raise RuntimeError("自启动入口是符号链接，未修改：" + str(path))
        return path.read_bytes() if path.exists() else None

    def discover(self):
        names = set()
        for directory in [self.directory] + self.system_dirs:
            for path in directory.glob("*.desktop"):
                if path.name == MANAGED:
                    continue
                try:
                    if is_sidera(path.name, path.read_bytes()):
                        names.add(path.name)
                except (OSError, UnicodeError, configparser.Error):
                    if path.name.lower().startswith("sidera"):
                        raise RuntimeError("无法确认自启动入口，未接管：" + str(path))
        plan = []
        for name in sorted(names):
            before = self.current(name)
            # A different user override already masks a lower-priority system entry.
            if before is not None and not is_sidera(name, before):
                continue
            after = b"[Desktop Entry]\nType=Application\nName=Sidera\nHidden=true\nX-Sidera-USB-Masked=true\n"
            plan.append({"name": name, "before": None if before is None else before.hex(),
                         "after": after.hex(), "mode": (self.directory / name).stat().st_mode & 0o777 if before is not None else 0o600})
        return plan

    def recover(self):
        if self.state().get("phase") in ("preparing", "restoring"):
            self.restore()
            self.recovered = True
            self.log("发现上次未完成的自启动切换，已恢复原有设置。")

    def enable(self):
        self.recover()
        if not (self.base / "manager.py").is_file():
            raise RuntimeError("请先完成离线安装，再设置自启动")
        state = self.state()
        if state.get("phase") == "active":
            for entry in state["entries"]:
                if self.current(entry["name"]) != bytes.fromhex(entry["after"]):
                    raise RuntimeError("旧版自启动被其他操作改动；请先恢复原版自启，再重新选择新版。")
            return self.set_enabled(True)
        if self.current(MANAGED) is not None:
            raise RuntimeError("同名受管理入口已经存在，未覆盖；请保留报告。")
        if self.directory.is_symlink() or self.directory.parent.is_symlink():
            raise RuntimeError("自启动目录不能是符号链接")
        self.directory.mkdir(parents=True, exist_ok=True)
        state = {"phase": "preparing", "enabled": True, "entries": self.discover()}
        self.save(self.journal, state)
        try:
            # Install recovery dispatcher first. If interrupted, it restores the
            # journal and starts the original; it never starts an uncommitted new app.
            self.write(self.directory / MANAGED, self.managed_bytes())
            for entry in state["entries"]:
                self.write(self.directory / entry["name"], bytes.fromhex(entry["after"]))
            state["phase"] = "active"
            self.save(self.journal, state)
        except Exception:
            self.restore()
            raise
        self.log("新版自启动已启用；旧版桌面自启已备份并停用，只保留一个有效入口。")

    def set_enabled(self, enabled):
        state = self.state()
        if state.get("phase") != "active":
            if enabled:
                return self.enable()
            self.log("当前没有接管自启动，原版自启保持原样。")
            return
        if self.current(MANAGED) not in (self.managed_bytes(True), self.managed_bytes(False)):
            raise RuntimeError("受管理自启动入口被外部修改，未覆盖")
        self.write(self.directory / MANAGED, self.managed_bytes(enabled))
        state["enabled"] = enabled
        self.save(self.journal, state)
        self.log("新版自启动已开启。" if enabled else "Sidera 自启动已关闭；恢复原版时可还原原先设置。")

    def restore(self):
        state = self.state()
        if state.get("phase") not in ("active", "preparing", "restoring"):
            return
        if self.directory.is_symlink() or self.directory.parent.is_symlink():
            raise RuntimeError("自启动目录已变为符号链接，未恢复或覆盖文件")
        # Inspect all files before restoring any: preserve external edits.
        for entry in state["entries"]:
            before = None if entry["before"] is None else bytes.fromhex(entry["before"])
            if self.current(entry["name"]) not in (before, bytes.fromhex(entry["after"])):
                raise RuntimeError("自启动文件被外部修改，未覆盖：" + entry["name"])
        if self.current(MANAGED) not in (None, self.managed_bytes(True), self.managed_bytes(False)):
            raise RuntimeError("受管理入口被外部修改，未覆盖")
        state["phase"] = "restoring"
        self.save(self.journal, state)
        for entry in state["entries"]:
            path = self.directory / entry["name"]
            if entry["before"] is None:
                if path.exists():
                    path.unlink()
            else:
                self.write(path, bytes.fromhex(entry["before"]))
                path.chmod(entry["mode"])
        managed = self.directory / MANAGED
        if managed.exists():
            managed.unlink()
        state["phase"] = "restored"
        state["enabled"] = False
        self.save(self.journal, state)
        self.log("原版自启动设置已原样恢复，新版自启动入口已撤回。")
