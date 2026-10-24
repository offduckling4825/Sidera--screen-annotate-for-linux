#!/usr/bin/env python3
"""Native layer-shell integration in private headless Sway, with XWayland disabled.

Requires sway, grim, bubblewrap, dbus-run-session, Python GI, GTK4 and GdkPixbuf.
Building the pointer helper also needs a C compiler, wayland-scanner and libwayland-dev;
alternatively supply --pointer-helper with an already built wayland_pointer.c.
Usage: python3 test_wayland_native.py /absolute/sidera [--unit-binary /absolute/test-binary]
No input, sockets, settings or network access reach the user's live desktop.
"""
import argparse
import json
import os
import select
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time

TARGET = r'''
import gi
gi.require_version("Gtk", "4.0")
from gi.repository import Gtk
import json
app = Gtk.Application(application_id="org.sidera.WaylandNativeTest")
def activate(application):
    window = Gtk.ApplicationWindow(application=application, title="Native Wayland receiver")
    window.set_child(Gtk.Label(label="Native Wayland receiver"))
    gesture = Gtk.GestureClick()
    for signal in ("pressed", "released"):
        gesture.connect(signal, lambda gesture, count, x, y, event=signal:
                        print(json.dumps({"event": event, "x": x, "y": y}), flush=True))
    window.add_controller(gesture)
    window.fullscreen()
    window.present()
app.connect("activate", activate)
app.run(None)
'''


def wait_for(check, description, timeout=10):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if check():
            return
        time.sleep(0.05)
    raise AssertionError("Timed out: " + description)


def stop(process):
    if process is not None and process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=4)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()


def inside(root):
    import gi
    gi.require_version("GdkPixbuf", "2.0")
    from gi.repository import GdkPixbuf
    assert os.environ.get("SIDERA_TEST_ISOLATED") == "1"
    env = os.environ | {"XDG_RUNTIME_DIR": str(root / "runtime"), "WLR_BACKENDS": "headless",
                        "WLR_RENDERER": "pixman", "WLR_HEADLESS_OUTPUTS": "1",
                        "XDG_SESSION_TYPE": "wayland", "XDG_CURRENT_DESKTOP": "sway",
                        "GDK_BACKEND": "wayland", "SIDERA_BACKEND": "wayland",
                        "SIDERA_NO_SPLASH": "1", "WPS_API_DEBUG": "0",
                        "DBUS_SYSTEM_BUS_ADDRESS": os.environ["DBUS_SESSION_BUS_ADDRESS"]}
    (root / "sway.conf").write_text(
        'xwayland disable\nseat seat0 fallback true\ndefault_border none\n'
        'output HEADLESS-1 mode 1920x1080\noutput * bg #e8edf4 solid_color\n')
    compositor = receiver = sidera = pointer = None
    logs = [(root / name).open("w") for name in ("sway.log", "receiver.log", "sidera.log")]
    result = {"passed": False, "cycles": [], "backend": "native Wayland; XWayland disabled"}
    try:
        compositor = subprocess.Popen(["sway", "-c", str(root / "sway.conf")], env=env,
                                      stdout=logs[0], stderr=logs[0])
        wait_for(lambda: list((root / "runtime").glob("sway-ipc.*.sock")), "Sway IPC")
        env["SWAYSOCK"] = str(next((root / "runtime").glob("sway-ipc.*.sock")))
        sockets = [p for p in (root / "runtime").glob("wayland-*") if p.is_socket()]
        assert len(sockets) == 1, sockets
        env["WAYLAND_DISPLAY"] = sockets[0].name
        env.pop("DISPLAY", None)

        pointer = subprocess.Popen([str(root / "pointer")], env=env, stdin=subprocess.PIPE,
                                   stdout=subprocess.PIPE, text=True, bufsize=1)
        def pointer_reply(expected):
            assert select.select([pointer.stdout], [], [], 5)[0], "Virtual pointer timed out"
            assert pointer.stdout.readline().strip() == expected, "Virtual pointer failed"
        pointer_reply("ready")
        extent = [1920, 1080]

        def ipc(*args):
            completed = subprocess.run(["swaymsg", "-r", *args], env=env,
                                       check=True, capture_output=True, text=True, timeout=5)
            value = json.loads(completed.stdout)
            if args[0] != "-t":
                assert all(item.get("success") for item in value), value
            return value

        def tree_has_receiver():
            return "Native Wayland receiver" in json.dumps(ipc("-t", "get_tree"))

        if (root / "unit-tests").exists():
            with (root / "buffers.log").open("w") as output:
                subprocess.run([str(root / "unit-tests"), "wayland::tests::", "--ignored",
                                "--test-threads=1"], env=env, stdout=output,
                               stderr=subprocess.STDOUT, check=True, timeout=30)
            result["buffer_protocol_tests"] = "passed"
        receiver = subprocess.Popen([sys.executable, "-c", TARGET], env=env,
                                    stdout=logs[1], stderr=logs[1])
        wait_for(tree_has_receiver, "native GTK receiver")
        sidera = subprocess.Popen([str(root / "sidera")], env=env,
                                  stdout=logs[2], stderr=logs[2])
        wait_for(lambda: "平台: wayland" in (root / "sidera.log").read_text(), "native backend")
        time.sleep(0.6)

        def motion(x, y):
            pointer.stdin.write(f"motion {x} {y} {extent[0]} {extent[1]}\n")
            pointer.stdin.flush()
            pointer_reply("ok")
            time.sleep(0.04)

        def button(pressed):
            pointer.stdin.write(f"button {int(pressed)}\n")
            pointer.stdin.flush()
            pointer_reply("ok")
            time.sleep(0.06)

        def click(x, y):
            motion(x, y)
            button(True)
            button(False)

        def counts():
            events = [json.loads(line)["event"] for line in (root / "receiver.log").read_text().splitlines()
                      if line.startswith("{")]
            return events.count("pressed"), events.count("released")

        def native_click(w, h):
            before = counts()
            click(w // 2, h - 50)
            expected = (before[0] + 1, before[1] + 1)
            wait_for(lambda: counts() == expected, f"native press/release {expected}", timeout=3)
            return counts()

        def snapshot(name):
            path = root / (name + ".png")
            subprocess.run(["grim", str(path)], env=env, check=True, timeout=5)
            pixbuf = GdkPixbuf.Pixbuf.new_from_file(str(path))
            data, stride, channels = pixbuf.get_pixels(), pixbuf.get_rowstride(), pixbuf.get_n_channels()
            # Compare only the middle canvas, away from toolbar hover and the cursor.
            ink = set()
            for y in range(40, pixbuf.get_height() - 80):
                for x in range(140, pixbuf.get_width() - 140):
                    i = y * stride + x * channels
                    r, g, b = data[i:i + 3]
                    if r > g + 45 and r > b + 45:
                        ink.add((x, y, r, g, b))
            return ink

        result["initial_click"] = native_click(1920, 1080)
        # Include real configure changes, fractional scaling, integer scaling,
        # and a return to the starting geometry. Each case redraws and passes through.
        for number, (width, height, scale) in enumerate([
            (1920, 1080, 1), (1280, 800, 1), (1600, 1000, 1.25),
            (1920, 1200, 1.5),
            (1920, 1200, 2), (1280, 800, 1),
        ]):
            ipc("output", "HEADLESS-1", "mode", f"{width}x{height}")
            ipc("output", "HEADLESS-1", "scale", str(scale))
            time.sleep(0.5)
            output = next(o for o in ipc("-t", "get_outputs") if o["active"])
            w, h = output["rect"]["width"], output["rect"]["height"]
            extent[:] = [w, h]
            # Startup preserves the initial anchor; subsequent resizes center
            # the expanded tools without the unlinked page counter.
            centered_height = 490 if number == 0 else 456
            sb_y = max(0, (h - centered_height) // 2)
            before = counts()
            click(w - 32, sb_y + 109)  # Pen: 12 + 2*(34+6) + 17.
            time.sleep(0.3)  # Let the 220 ms tool drawer finish before later UI input.
            motion(w // 3, h // 2 - 50)
            button(True)
            for step in range(1, 7):
                motion(w // 3 + step * 15, h // 2 - 50 + step * 7)
            button(False)
            time.sleep(0.2)
            assert counts() == before, "Drawing leaked to the native window"
            ink = snapshot(f"cycle-{number}-pen")
            assert len(ink) > 20, "No visible ink (first configure or scaled drawing failed)"
            click(w - 32, sb_y + 69)  # Cursor.
            time.sleep(0.2)
            assert snapshot(f"cycle-{number}-cursor") == ink, "Switching mode changed ink pixels"
            result["cycles"].append({"physical": [width, height], "logical": [w, h],
                                     "scale": scale, "ink_pixels": len(ink),
                                     "native_click": native_click(w, h)})
        assert sidera.poll() is None and compositor.poll() is None, "Unexpected process exit"
        result["passed"] = True
        print("PASS: native Wayland drawing, passthrough, resize and scale")
        return 0
    except Exception as error:
        result["error"] = str(error)
        print(f"FAIL: {error}", file=sys.stderr)
        return 1
    finally:
        for process in (pointer, sidera, receiver, compositor):
            stop(process)
        for log in logs:
            log.close()
        (root / "result.json").write_text(json.dumps(result, indent=2) + "\n")


def main():
    if len(sys.argv) == 3 and sys.argv[1] == "--inside":
        return inside(Path(sys.argv[2]))
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--unit-binary", type=Path)
    parser.add_argument("--pointer-helper", type=Path, help="prebuilt wayland_pointer.c helper")
    parser.add_argument("--output-dir", type=Path)
    args = parser.parse_args()
    for command in ("bwrap", "dbus-run-session", "sway", "swaymsg", "grim"):
        if shutil.which(command) is None:
            parser.error(f"Missing dependency: {command}")
    root = args.output_dir.resolve() if args.output_dir else Path(tempfile.mkdtemp(prefix="sidera-native-", dir="/var/tmp"))
    if args.output_dir:
        root.mkdir(parents=True, exist_ok=False)
    for name in ("home", "runtime"):
        (root / name).mkdir(mode=0o700)
    shutil.copy2(args.binary.resolve(), root / "sidera")
    if args.unit_binary:
        shutil.copy2(args.unit_binary.resolve(), root / "unit-tests")
    shutil.copy2(__file__, root / "runner.py")
    if args.pointer_helper:
        shutil.copy2(args.pointer_helper.resolve(), root / "pointer")
    else:
        source = Path(__file__).resolve()
        protocol = source.parents[2] / "protocols/wlr-virtual-pointer-unstable-v1.xml"
        for kind, output in (("client-header", "virtual-pointer.h"), ("private-code", "virtual-pointer.c")):
            subprocess.run(["wayland-scanner", kind, str(protocol), str(root / output)], check=True)
        subprocess.run(["cc", "-Wall", "-Wextra", "-Werror", "-I", str(root),
                        str(source.with_name("wayland_pointer.c")), str(root / "virtual-pointer.c"),
                        "-lwayland-client", "-o", str(root / "pointer")], check=True)
    (root / "dbus.conf").write_text(
        '<busconfig><type>session</type><listen>unix:tmpdir=/tmp</listen>'
        '<auth>EXTERNAL</auth><apparmor mode="disabled"/>'
        '<policy context="default"><allow send_destination="*"/>'
        '<allow receive_sender="*"/><allow own="*"/></policy></busconfig>\n')
    env = {key: value for key, value in os.environ.items()
           if not key.startswith(("SIDERA_", "XDG_", "WLR_")) and key not in (
               "DISPLAY", "WAYLAND_DISPLAY", "WAYLAND_SOCKET", "XAUTHORITY",
               "SWAYSOCK", "I3SOCK", "DBUS_SESSION_BUS_ADDRESS", "DBUS_SYSTEM_BUS_ADDRESS")}
    env["SIDERA_TEST_ISOLATED"] = "1"
    print(f"Isolated test artifacts: {root}", flush=True)
    with (root / "harness.log").open("w") as log:
        result = subprocess.run([
            "bwrap", "--die-with-parent", "--unshare-pid", "--unshare-net",
            "--ro-bind", "/", "/", "--dev", "/dev", "--proc", "/proc",
            "--tmpfs", "/tmp", "--tmpfs", "/run",
            "--bind", str(root / "home"), str(Path.home()), "--bind", str(root), str(root),
            "--chdir", str(root), "dbus-run-session", f"--config-file={root / 'dbus.conf'}", "--",
            sys.executable, str(root / "runner.py"), "--inside", str(root),
        ], env=env, stdout=log, stderr=subprocess.STDOUT, timeout=120)
    print((root / "harness.log").read_text(), end="")
    return result.returncode


if __name__ == "__main__":
    raise SystemExit(main())
