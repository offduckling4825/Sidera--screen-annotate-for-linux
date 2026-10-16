#!/usr/bin/env python3
"""Run the built Rust executable in an isolated home and Xvfb display.

Usage: python3 rust/tests/test_startup.py /absolute/path/to/sidera
Requires Linux, bubblewrap and Xvfb. No real desktop or WPS registration is used.
"""

import os
from pathlib import Path
import select
import shutil
import socket
import subprocess
import sys
import tempfile
import time
import unittest


BINARY = Path(sys.argv.pop(1)).resolve() if len(sys.argv) > 1 else None


class StartupTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if BINARY is None or not BINARY.is_file():
            raise RuntimeError("Pass the path of a built Sidera executable")
        for command in ("bwrap", "Xvfb"):
            if shutil.which(command) is None:
                raise RuntimeError(f"Missing test dependency: {command}")
        read_fd, write_fd = os.pipe()
        cls.server_log = tempfile.TemporaryFile()
        cls.addClassCleanup(cls.server_log.close)
        cls.server = subprocess.Popen(
            ["Xvfb", "-displayfd", str(write_fd), "-screen", "0", "1024x768x24", "-nolisten", "tcp"],
            pass_fds=(write_fd,), stdout=subprocess.DEVNULL, stderr=cls.server_log,
        )
        cls.addClassCleanup(cls.stop_server)
        os.close(write_fd)
        try:
            if not select.select([read_fd], [], [], 10)[0]:
                raise RuntimeError("Xvfb did not start within 10 seconds")
            number = os.read(read_fd, 32).decode().strip()
            if not number.isdigit():
                cls.server_log.seek(0)
                raise RuntimeError("Xvfb failed to allocate a display: " + cls.server_log.read(2000).decode(errors="replace"))
            cls.display = ":" + number
        finally:
            os.close(read_fd)

    @classmethod
    def stop_server(cls):
        cls.server.terminate()
        try:
            cls.server.wait(timeout=5)
        except subprocess.TimeoutExpired:
            cls.server.kill()
            cls.server.wait(timeout=5)

    def setUp(self):
        scratch = tempfile.TemporaryDirectory(prefix="sidera-startup-test-")
        self.addCleanup(scratch.cleanup)
        self.root = Path(scratch.name)
        for directory in ("home", "runtime", "state"):
            (self.root / directory).mkdir(mode=0o700)
        self.env = {key: value for key, value in os.environ.items()
                    if not key.startswith("SIDERA_") and key not in (
                        "DISPLAY", "WAYLAND_DISPLAY", "WAYLAND_SOCKET", "XDG_SESSION_TYPE",
                        "XAUTHORITY", "DBUS_SESSION_BUS_ADDRESS", "XDG_CONFIG_HOME",
                        "XDG_DATA_HOME", "XDG_CACHE_HOME",
                    )}
        self.env.update(
            DISPLAY=self.display, XDG_RUNTIME_DIR=str(self.root / "runtime"),
            XDG_STATE_HOME=str(self.root / "state"), SIDERA_SMOKE="1",
            SIDERA_NO_STARTUP_NOTIFICATION="1", WPS_API_DEBUG="0", RUST_LOG="info",
        )

    def launch(self, **changes):
        return subprocess.run(
            ["bwrap", "--die-with-parent", "--ro-bind", "/", "/", "--dev", "/dev",
             "--proc", "/proc", "--bind", str(self.root), str(self.root),
             "--bind", str(self.root / "home"), str(Path.home()), "--chdir", "/tmp",
             str(BINARY)], env=self.env | changes, stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT, text=True, encoding="utf-8", timeout=15,
        )

    def assert_failure(self, result, expected):
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn(expected, result.stdout)
        self.assertIn(expected, (self.root / "state/sidera/startup-error.log").read_text())

    def assert_rendered(self, result):
        self.assertEqual(result.returncode, 0, result.stdout)
        self.assertIn("[SMOKE]", result.stdout)
        self.assertIn("平台: x11", result.stdout)
        self.assertFalse((self.root / "state/sidera/startup-error.log").exists())

    def test_x11_session_ignores_stale_wayland_display(self):
        self.assert_rendered(self.launch(XDG_SESSION_TYPE="x11", WAYLAND_DISPLAY="missing"))

    def test_empty_wayland_display_is_not_a_wayland_session(self):
        self.assert_rendered(self.launch(WAYLAND_DISPLAY=""))

    def test_explicit_x11_override(self):
        self.assert_rendered(self.launch(SIDERA_BACKEND="x11", XDG_SESSION_TYPE="wayland"))

    def test_explicit_wayland_failure_does_not_silently_use_x11(self):
        result = self.launch(SIDERA_BACKEND="wayland", XDG_SESSION_TYPE="x11", WAYLAND_DISPLAY="missing")
        self.assert_failure(result, "无法连接 Wayland")
        self.assertNotIn("平台: x11", result.stdout)

    def test_auto_wayland_failure_does_not_silently_use_x11(self):
        result = self.launch(XDG_SESSION_TYPE="wayland", WAYLAND_DISPLAY="missing")
        self.assert_failure(result, "无法连接 Wayland")
        self.assertNotIn("平台: x11", result.stdout)

    def test_invalid_backend_is_reported(self):
        self.assert_failure(self.launch(SIDERA_BACKEND="typo"), "SIDERA_BACKEND 必须是")

    def test_no_display_fails_with_diagnostic(self):
        self.assert_failure(self.launch(DISPLAY="", XDG_SESSION_TYPE="x11"), "无法连接 X11")

    def test_unwritable_instance_socket_is_not_success(self):
        (self.root / "runtime/sidera.sock").mkdir()
        self.assert_failure(self.launch(), "无法创建单实例通信文件")

    def test_running_instance_still_wakes_successfully(self):
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as listener:
            listener.bind(str(self.root / "runtime/sidera.sock"))
            listener.listen(1)
            listener.settimeout(5)
            result = self.launch()
            self.assertEqual(result.returncode, 0, result.stdout)
            connection, _ = listener.accept()
            with connection:
                self.assertEqual(connection.recv(16), b"show")
            self.assertIn("Sidera 已在运行", result.stdout)
            self.assertNotIn("[SMOKE]", result.stdout)

    def test_failed_log_write_keeps_original_failure(self):
        (self.root / "state/sidera").write_text("blocks the directory")
        result = self.launch(SIDERA_BACKEND="typo")
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn("SIDERA_BACKEND 必须是", result.stdout)
        self.assertIn("无法保存启动诊断", result.stdout)

    def notification_helper(self, script):
        directory = self.root / "bin"
        directory.mkdir()
        helper = directory / "notify-send"
        helper.write_text("#!/bin/sh\n" + script)
        helper.chmod(0o700)
        return {"SIDERA_NO_STARTUP_NOTIFICATION": "0", "PATH": str(directory) + ":" + os.defpath}

    def test_notification_is_delivered_before_process_exits(self):
        marker = self.root / "notification.txt"
        environment = self.notification_helper(f'/bin/sleep 0.15\nprintf "%s\\n" "$@" > "{marker}"\n')
        self.assert_failure(self.launch(SIDERA_BACKEND="typo", **environment), "SIDERA_BACKEND 必须是")
        self.assertIn("Sidera 无法启动", marker.read_text())
        self.assertIn("--urgency=critical", marker.read_text())

    def test_unresponsive_notification_does_not_block_exit(self):
        environment = self.notification_helper("exec /bin/sleep 30\n")
        started = time.monotonic()
        self.assert_failure(self.launch(SIDERA_BACKEND="typo", **environment), "SIDERA_BACKEND 必须是")
        self.assertLess(time.monotonic() - started, 6)


if __name__ == "__main__":
    unittest.main(verbosity=2)
