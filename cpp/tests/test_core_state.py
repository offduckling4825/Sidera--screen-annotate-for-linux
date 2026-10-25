#!/usr/bin/env python3
"""Build legacy backends and check their shared state in private headless Sway."""
from pathlib import Path
import argparse
import shlex
import subprocess
import sys
import tempfile


def main():
    repo = Path(__file__).resolve().parents[2]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sanitize", action="store_true", help="also instrument native code with ASan/UBSan")
    args = parser.parse_args()
    instrumentation = ["-fsanitize=address,undefined", "-fno-omit-frame-pointer"] if args.sanitize else []
    make = ["make", "-j2" if args.sanitize else "-j4", "WITH_WAYLAND=1"]
    if args.sanitize:
        make += ["CXXFLAGS=-std=c++17 -O1 -g1 -fPIC " + " ".join(instrumentation), "LDFLAGS=-fsanitize=address,undefined"]
    subprocess.run(make, cwd=repo / "cpp", check=True)
    flags = shlex.split(subprocess.check_output(["pkg-config", "--cflags", "--libs",
        "Qt5Widgets", "Qt5Network", "wayland-client", "xkbcommon"], text=True))
    objects = [path for path in (repo / "cpp/src").glob("*.o") if path.name != "main.o"]
    objects += list((repo / "cpp/build").glob("*.o"))
    with tempfile.TemporaryDirectory(prefix="sidera-cpp-core-", dir="/var/tmp") as temporary:
        binary = Path(temporary) / "core-state"
        subprocess.run(["g++", "-std=c++17", "-O2", "-fPIC", "-I" + str(repo / "cpp/src"),
            "-I" + str(repo / "cpp/build"), str(repo / "cpp/tests/core_state.cpp"),
            *map(str, objects), *flags, *instrumentation, "-lX11", "-lxcb", "-lXext", "-lXtst", "-o", str(binary)], check=True)
        command = [sys.executable, str(repo / "tools/run_wayland_isolated.py"), "--"]
        if args.sanitize:
            # Qt/platform process-global allocations are outside this lifecycle fixture.
            command += ["env", "ASAN_OPTIONS=detect_leaks=0:halt_on_error=1", "UBSAN_OPTIONS=halt_on_error=1:print_stacktrace=1"]
        return subprocess.run(command + [str(binary)], check=False).returncode


if __name__ == "__main__":
    raise SystemExit(main())
