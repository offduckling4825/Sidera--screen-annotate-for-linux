#!/usr/bin/env python3
"""Build locked Rust tests, then execute only in a private home/network/desktop."""
from pathlib import Path
import argparse
import json
import subprocess
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    backend = parser.add_mutually_exclusive_group()
    backend.add_argument("--x11", action="store_true", help="run ignored X11 protocol tests in private Xvfb")
    backend.add_argument("--wayland", action="store_true", help="run ignored layer-shell checks in private headless Sway")
    parser.add_argument("filter", nargs="?", default="")
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[1]
    build = subprocess.run(["cargo", "test", "--locked", "--no-run", "--message-format=json"],
                           cwd=repo / "rust", text=True, stdout=subprocess.PIPE, check=False)
    executables = []
    for line in build.stdout.splitlines():
        record = json.loads(line)
        if record.get("reason") == "compiler-message":
            print(record["message"].get("rendered", ""), file=sys.stderr, end="")
        if record.get("reason") == "compiler-artifact" and record.get("profile", {}).get("test") and record.get("executable"):
            executables.append(record["executable"])
    if build.returncode:
        return build.returncode
    if not executables:
        raise RuntimeError("No test executable was produced; refusing an empty successful run")
    for executable in sorted(set(executables)):
        command = [sys.executable, str(repo / "tools/run_isolated.py"), "--timeout", "300", "--"]
        if args.wayland:
            command = [sys.executable, str(repo / "tools/run_wayland_isolated.py"), "--",
                       executable, "wayland::tests::", "--ignored", "--test-threads=1"]
        elif args.x11:
            command += ["xvfb-run", "-a", executable, "x11::", "--ignored", "--test-threads=1"]
        else:
            command += [executable, args.filter, "--test-threads=2"]
        result = subprocess.run(command, cwd=repo / "rust", check=False)
        if result.returncode:
            return result.returncode
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
