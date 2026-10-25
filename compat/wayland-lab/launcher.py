#!/usr/bin/python3
"""Open a private nested Sway desktop, or close only that desktop with --stop."""
import fcntl
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time


def notify(message):
    print(message, flush=True)
    if shutil.which("notify-send"):
        subprocess.run(["notify-send", "Sidera Wayland 测试", message], timeout=5)


def main():
    state = Path.home() / ".local/state/sidera-wayland-lab"
    state.mkdir(parents=True, exist_ok=True, mode=0o700)
    current = state / "current-runtime"
    if "--stop" in sys.argv:
        sockets = list(current.glob("sway-ipc.*.sock")) if current.exists() else []
        for socket in sockets:
            subprocess.run(["swaymsg", "-s", str(socket), "exit"], timeout=5)
        if not sockets:
            notify("测试桌面当前没有运行。")
        return 0
    for executable in ("bwrap", "dbus-run-session", "sway", "swaymsg", "sidera"):
        if not shutil.which(executable):
            notify(f"缺少测试组件：{executable}")
            return 1
    outer_runtime = Path(os.environ.get("XDG_RUNTIME_DIR", f"/run/user/{os.getuid()}"))
    display = os.environ.get("WAYLAND_DISPLAY", "")
    parent_socket = Path(display) if display.startswith("/") else outer_runtime / display
    if not display or not parent_socket.is_socket():
        notify("请从 Ubuntu 的 Wayland 桌面打开这个测试入口。")
        return 1
    with (state / "launcher.lock").open("w") as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            notify("测试桌面已经打开，请切换到 Sway 窗口。")
            return 0
        run = state / (time.strftime("%Y%m%d-%H%M%S") + f"-{os.getpid()}")
        run.mkdir(mode=0o700)
        for name in ("home", "runtime"):
            (run / name).mkdir(mode=0o700)
        if current.is_symlink():
            current.unlink()
        current.symlink_to(run / "runtime", target_is_directory=True)
        # Private bus and home prevent test settings, instance locks, and WPS's
        # local bridge from reaching the user's normal applications.
        (run / "dbus.conf").write_text(
            '<busconfig><type>session</type><listen>unix:tmpdir=/tmp</listen>'
            '<auth>EXTERNAL</auth><apparmor mode="disabled"/>'
            '<policy context="default"><allow send_destination="*"/>'
            '<allow receive_sender="*"/><allow own="*"/></policy></busconfig>\n')
        env = {key: value for key, value in os.environ.items()
               if not key.startswith(("XDG_", "SIDERA_", "WLR_")) and key not in (
                   "DISPLAY", "WAYLAND_DISPLAY", "WAYLAND_SOCKET", "XAUTHORITY",
                   "SWAYSOCK", "I3SOCK", "DBUS_SESSION_BUS_ADDRESS",
                   "DBUS_SYSTEM_BUS_ADDRESS", "SESSION_MANAGER")}
        env.update(XDG_RUNTIME_DIR=str(outer_runtime), WAYLAND_DISPLAY="parent-wayland",
                   XDG_SESSION_TYPE="wayland", XDG_CURRENT_DESKTOP="sway",
                   WLR_BACKENDS="wayland", WLR_RENDERER="pixman", GDK_BACKEND="wayland",
                   SIDERA_LAB_ROOT="/tmp/sidera-lab")
        command = [
            "bwrap", "--die-with-parent", "--unshare-pid", "--unshare-net",
            "--ro-bind", "/", "/", "--dev", "/dev", "--proc", "/proc",
            "--tmpfs", "/tmp", "--tmpfs", "/run",
            "--bind", str(run / "home"), str(Path.home()),
            "--bind", str(run), "/tmp/sidera-lab",
            "--bind", str(run / "runtime"), str(outer_runtime),
            "--ro-bind", str(parent_socket), str(outer_runtime / "parent-wayland"),
            "--chdir", str(Path.home()), "dbus-run-session",
            "--config-file=/tmp/sidera-lab/dbus.conf", "--",
            "sway", "-c", "/usr/local/share/sidera-wayland-lab/sway.conf",
        ]
        with (run / "desktop.log").open("w") as log:
            result = subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT)
        if current.is_symlink():
            current.unlink()
        if result.returncode:
            notify(f"测试桌面启动失败，日志：{run / 'desktop.log'}")
        return result.returncode


if __name__ == "__main__":
    raise SystemExit(main())
