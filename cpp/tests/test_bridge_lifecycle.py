#!/usr/bin/env python3
"""Compile and test the real Qt bridge in an isolated home and network.

Run from any directory on Linux. Requires g++, pkg-config, Qt5Core/Network
development files and bubblewrap. No production service or WPS is needed.
"""

from pathlib import Path
import shlex
import shutil
import subprocess
import tempfile


def main():
    repo = Path(__file__).resolve().parents[2]
    for command in ("g++", "pkg-config", "bwrap"):
        if shutil.which(command) is None:
            raise RuntimeError(f"Missing required test dependency: {command}")
    flags = shlex.split(subprocess.check_output(
        ["pkg-config", "--cflags", "--libs", "Qt5Core", "Qt5Network"], text=True,
    ))
    with tempfile.TemporaryDirectory(prefix="sidera-lifecycle-", dir="/var/tmp") as folder:
        scratch = Path(folder)
        binary = scratch / "bridge-lifecycle"
        isolated_home = scratch / "home"
        isolated_home.mkdir()
        subprocess.run([
            "g++", "-std=c++17", "-O2", "-fPIC", "-I", str(repo / "cpp/src"),
            str(repo / "cpp/tests/bridge_lifecycle.cpp"), str(repo / "cpp/src/wps_bridge.cpp"),
            "-o", str(binary), *flags,
        ], check=True)
        result = subprocess.run([
            "bwrap", "--die-with-parent", "--ro-bind", "/", "/",
            "--dev", "/dev", "--proc", "/proc", "--tmpfs", "/tmp",
            "--bind", str(isolated_home), str(Path.home()),
            # Keep test files visible even when the checkout is under the real home.
            "--ro-bind", str(repo), str(repo),
            "--unshare-net", "--chdir", str(scratch), str(binary),
        ], text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=30)
        print(result.stdout, end="")
        if result.returncode:
            print("\n".join(result.stderr.splitlines()[-30:]))
        return result.returncode


if __name__ == "__main__":
    raise SystemExit(main())
