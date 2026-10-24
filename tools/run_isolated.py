#!/usr/bin/env python3
"""Run code checks with a temporary home, private network and no desktop session."""
from pathlib import Path
import argparse
import shutil
import subprocess
import sys
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--timeout", type=int, default=180)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command
    if command[:1] == ["--"]:
        command = command[1:]
    if not command:
        parser.error("provide a command after --")
    if sys.platform != "linux" or shutil.which("bwrap") is None:
        parser.error("Linux and bubblewrap are required; refusing an unisolated fallback")
    repo = Path(__file__).resolve().parents[1]
    with tempfile.TemporaryDirectory(prefix="sidera-isolated-", dir="/var/tmp") as temporary:
        home = Path(temporary) / "home"
        home.mkdir(mode=0o700)
        options = [
            "bwrap", "--die-with-parent", "--new-session",
            "--ro-bind", "/", "/", "--dev", "/dev", "--proc", "/proc",
            "--tmpfs", "/tmp", "--tmpfs", "/run", "--tmpfs", "/home",
            "--bind", str(home), "/tmp/home",
            "--ro-bind", str(repo), str(repo),
            "--dir", "/tmp/runtime", "--chmod", "0700", "/tmp/runtime",
            "--unshare-net", "--unshare-pid", "--clearenv",
            "--setenv", "PATH", "/usr/bin:/bin",
            "--setenv", "HOME", "/tmp/home",
            "--setenv", "XDG_CONFIG_HOME", "/tmp/home/.config",
            "--setenv", "XDG_DATA_HOME", "/tmp/home/.local/share",
            "--setenv", "XDG_CACHE_HOME", "/tmp/home/.cache",
            "--setenv", "XDG_STATE_HOME", "/tmp/home/.local/state",
            "--setenv", "XDG_RUNTIME_DIR", "/tmp/runtime",
            "--setenv", "LANG", "C.UTF-8",
            "--setenv", "NO_AT_BRIDGE", "1",
            "--setenv", "RUST_TEST_THREADS", "2",
            "--chdir", str(Path.cwd()),
        ]
        try:
            return subprocess.run(options + command, timeout=args.timeout, check=False).returncode
        except subprocess.TimeoutExpired:
            print(f"Isolated check exceeded {args.timeout}s", file=sys.stderr)
            return 124


if __name__ == "__main__":
    raise SystemExit(main())
