#!/usr/bin/env python3
"""Run protocol checks in private headless Sway, with no live desktop access."""
from pathlib import Path
import os
import shutil
import subprocess
import sys
import tempfile
import time


def inside(root, command):
    root = Path(root)
    env = {**os.environ, "WLR_BACKENDS": "headless", "WLR_RENDERER": "pixman",
           "WLR_HEADLESS_OUTPUTS": "1", "XDG_SESSION_TYPE": "wayland",
           "XDG_CURRENT_DESKTOP": "sway", "QT_QPA_PLATFORM": "offscreen"}
    config = root / "sway.conf"
    config.write_text('xwayland disable\nseat seat0 fallback true\noutput HEADLESS-1 mode 800x600\n')
    with (root / "sway.log").open("w") as log:
        compositor = subprocess.Popen(["sway", "-c", str(config)], env=env, stdout=log, stderr=log)
        try:
            deadline = time.monotonic() + 10
            runtime = root / "runtime"
            while True:
                displays = [p for p in runtime.glob("wayland-*") if p.is_socket()]
                sockets = list(runtime.glob("sway-ipc.*.sock"))
                if displays and sockets:
                    break
                if compositor.poll() is not None or time.monotonic() > deadline:
                    raise RuntimeError("Private Sway did not start:\n" + (root / "sway.log").read_text()[-4000:])
                time.sleep(0.05)
            env.update(WAYLAND_DISPLAY=displays[0].name, SWAYSOCK=str(sockets[0]))
            return subprocess.run(command, env=env, timeout=180, check=False).returncode
        finally:
            compositor.terminate()
            try:
                compositor.wait(timeout=5)
            except subprocess.TimeoutExpired:
                compositor.kill(); compositor.wait()


def main():
    if sys.argv[1:2] == ["--inside"]:
        assert os.environ.get("SIDERA_TEST_ISOLATED") == "1"
        return inside(sys.argv[2], sys.argv[3:])
    command = sys.argv[1:]
    if command[:1] == ["--"]: command = command[1:]
    if not command: raise RuntimeError("Provide a test command after --")
    for tool in ("bwrap", "sway", "dbus-run-session"):
        if not shutil.which(tool): raise RuntimeError("Missing required dependency: " + tool)
    repo = Path(__file__).resolve().parents[1]
    with tempfile.TemporaryDirectory(prefix="sidera-wayland-code-", dir="/var/tmp") as temporary:
        root = Path(temporary)
        for name in ("home", "runtime"): (root / name).mkdir(mode=0o700)
        args = ["bwrap", "--die-with-parent", "--new-session", "--unshare-net", "--unshare-pid",
                "--ro-bind", "/", "/", "--dev", "/dev", "--proc", "/proc",
                "--tmpfs", "/tmp", "--tmpfs", "/run", "--tmpfs", "/home",
                "--bind", str(root), str(root), "--ro-bind", str(repo), str(repo), "--clearenv"]
        env = {"PATH": "/usr/bin:/bin", "HOME": str(root / "home"),
               "XDG_RUNTIME_DIR": str(root / "runtime"), "XDG_CONFIG_HOME": str(root / "home/.config"),
               "XDG_DATA_HOME": str(root / "home/.local/share"), "LANG": "C.UTF-8",
               "SIDERA_TEST_ISOLATED": "1", "NO_AT_BRIDGE": "1"}
        for key, value in env.items(): args += ["--setenv", key, value]
        args += ["dbus-run-session", "--", sys.executable, str(Path(__file__).resolve()), "--inside", str(root), *command]
        return subprocess.run(args, timeout=210, check=False).returncode


if __name__ == "__main__":
    raise SystemExit(main())
