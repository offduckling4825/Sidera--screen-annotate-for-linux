#!/usr/bin/env python3
"""Manual GNOME Wayland integration test; never connects to the current desktop.

Usage: python3 rust/tests/test_mutter_passthrough.py /absolute/path/to/sidera
Requires Linux, bubblewrap, dbus-run-session, GNOME Shell with headless support,
Xwayland, Python GObject introspection, and the GTK 4 introspection package.
Tested with GNOME/Mutter 50.1. This optional test is not part of ordinary CI.

A private PID/network namespace, home, /tmp, /run, runtime directory and session
bus contain a headless GNOME compositor and a native Wayland GTK click receiver.
Clutter virtual pointer events exercise compositor picking; XTest is not used.
Two pen -> drawing -> cursor cycles must deliver complete clicks to GTK.
Logs remain in the printed temporary directory, or in --output-dir DIRECTORY.
Add --check-stable-visual to require the split input/visual overlay, preserve
ink pixels across each switch, reject visual remapping, and exercise dragging.
That check also requires libX11 and libXcomposite (normally installed by GNOME).
"""

import argparse
import ctypes as C
import ctypes.util
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import time


TARGET_TITLE = "Sidera native input test"
TARGET_CODE = r"""
import gi
gi.require_version("Gtk", "4.0")
from gi.repository import Gtk
import json

app = Gtk.Application(application_id="org.sidera.NativeInputTest")
def activate(application):
    window = Gtk.ApplicationWindow(application=application,
                                   title="Sidera native input test")
    window.set_child(Gtk.Label(label="Native Wayland click receiver"))
    gesture = Gtk.GestureClick()
    for signal in ("pressed", "released"):
        gesture.connect(signal, lambda gesture, count, x, y, event=signal:
                        print(json.dumps({"event": event, "x": x, "y": y}),
                              flush=True))
    window.add_controller(gesture)
    window.fullscreen()
    window.present()
app.connect("activate", activate)
app.run(None)
"""


class VisualProbe:
    """Observe the permanent visual without manipulating or refreshing it."""

    def __init__(self, env):
        os.environ.update({name: env[name] for name in ("DISPLAY", "XAUTHORITY")})
        self.x = C.CDLL(ctypes.util.find_library("X11"))
        self.composite = C.CDLL(ctypes.util.find_library("Xcomposite"))
        d, w, i = C.c_void_p, C.c_ulong, C.c_int
        signatures = {
            "XOpenDisplay": ([C.c_char_p], d),
            "XDefaultRootWindow": ([d], w),
            "XInternAtom": ([d, C.c_char_p, i], w),
            "XQueryTree": ([d, w, C.POINTER(w), C.POINTER(w),
                            C.POINTER(C.POINTER(w)), C.POINTER(C.c_uint)], i),
            "XGetWindowProperty": ([d, w, w, C.c_long, C.c_long, i, w,
                                    C.POINTER(w), C.POINTER(i), C.POINTER(w),
                                    C.POINTER(w), C.POINTER(d)], i),
            "XSelectInput": ([d, w, C.c_long], i),
            "XPending": ([d], i), "XNextEvent": ([d, d], i),
            "XSync": ([d, i], i), "XFree": ([d], i),
            "XGetImage": ([d, w, i, i, C.c_uint, C.c_uint, w, i], d),
            "XGetPixel": ([d, i, i], w), "XDestroyImage": ([d], i),
            "XFreePixmap": ([d, w], i), "XCloseDisplay": ([d], i),
        }
        for name, (arguments, result) in signatures.items():
            function = getattr(self.x, name)
            function.argtypes, function.restype = arguments, result
        self.composite.XCompositeNameWindowPixmap.argtypes = [d, w]
        self.composite.XCompositeNameWindowPixmap.restype = w
        self.display = self.x.XOpenDisplay(env["DISPLAY"].encode())
        if not self.display:
            raise RuntimeError("Could not open the isolated X11 display")
        root = self.x.XDefaultRootWindow(self.display)
        parent, returned_root, count = w(), w(), C.c_uint()
        children = C.POINTER(w)()
        self.x.XQueryTree(self.display, root, C.byref(returned_root), C.byref(parent),
                          C.byref(children), C.byref(count))
        role_atom = self.x.XInternAtom(self.display, b"_SIDERA_OVERLAY_ROLE", 1)
        if not role_atom:
            self.x.XFree(children)
            self.close()
            raise AssertionError("The application does not label split input/visual windows")
        roles = {}
        try:
            for window in children[:count.value]:
                actual, format_, items, remaining, data = w(), i(), w(), w(), d()
                self.x.XGetWindowProperty(
                    self.display, window, role_atom, 0, 32, 0, 0, C.byref(actual),
                    C.byref(format_), C.byref(items), C.byref(remaining), C.byref(data))
                if data:
                    if format_.value == 8:
                        roles[C.string_at(data, items.value).decode("ascii", "replace")] = window
                    self.x.XFree(data)
        finally:
            self.x.XFree(children)
        if "visual" not in roles or "input" not in roles:
            self.close()
            raise AssertionError("Expected _SIDERA_OVERLAY_ROLE visual and input windows")
        self.window = roles["visual"]
        self.x.XSelectInput(self.display, self.window, 1 << 17)  # StructureNotify
        self.x.XSync(self.display, 0)

    def mapping_events(self):
        events = []
        while self.x.XPending(self.display):
            event = (C.c_long * 24)()
            self.x.XNextEvent(self.display, C.byref(event))
            kind = C.cast(event, C.POINTER(C.c_int))[0]
            if kind in (17, 18, 19):  # Destroy, Unmap, Map
                events.append(kind)
        return events

    def ink(self):
        # Read the redirected backing pixmap. XGetImage(window) can return
        # undefined pixels when the transparent input window obscures it.
        pixmap = self.composite.XCompositeNameWindowPixmap(self.display, self.window)
        image = self.x.XGetImage(self.display, pixmap, 360, 320, 240, 240,
                                C.c_ulong(-1), 2)
        try:
            if not image:
                raise AssertionError("Could not read the visual's redirected pixmap")
            return [self.x.XGetPixel(image, x, y) for y in range(240) for x in range(240)]
        finally:
            if image:
                self.x.XDestroyImage(image)
            self.x.XFreePixmap(self.display, pixmap)

    def close(self):
        if self.display:
            self.x.XCloseDisplay(self.display)
            self.display = None


def stop(process):
    if process is None or process.poll() is not None:
        return
    process.terminate()
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=5)


def wait_for(condition, description, timeout=5):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if condition():
            return
        time.sleep(0.05)
    raise AssertionError(f"Timed out: {description}")


def inside(root):
    if os.environ.get("SIDERA_TEST_ISOLATED") != "1":
        raise RuntimeError("The internal runner must be launched by its isolation wrapper")
    import gi
    from gi.repository import Gio, GLib

    env = dict(os.environ)
    env.update(XDG_RUNTIME_DIR=str(root / "runtime"),
               WAYLAND_DISPLAY="sidera-input-test", XDG_SESSION_TYPE="wayland",
               LIBGL_ALWAYS_SOFTWARE="1", GSETTINGS_BACKEND="memory",
               GTK_A11Y="none", WPS_API_DEBUG="0", SIDERA_NO_SPLASH="1",
               SIDERA_NO_STARTUP_NOTIFICATION="1", SIDERA_TRACE="1")
    # Shell needs an available bus connection even when system services are
    # absent. Route those lookups to the private test bus, never the host bus.
    env["DBUS_SYSTEM_BUS_ADDRESS"] = env["DBUS_SESSION_BUS_ADDRESS"]
    for name in ("DISPLAY", "XAUTHORITY", "WAYLAND_SOCKET", "XDG_SESSION_ID"):
        env.pop(name, None)
    compositor = receiver = sidera = probe = bus = None
    result = {"passed": False, "cycles": []}
    compositor_log = (root / "gnome.log").open("w")
    receiver_log = (root / "receiver.log").open("w")
    sidera_log = (root / "sidera.log").open("w")
    try:
        compositor = subprocess.Popen(
            ["gnome-shell", "--wayland", "--headless", "--unsafe-mode",
             "--virtual-monitor=1280x800", "--wayland-display=sidera-input-test"],
            env=env, stdout=compositor_log, stderr=compositor_log)

        def display_ready():
            if compositor.poll() is not None:
                raise RuntimeError("Isolated GNOME exited; see gnome.log")
            return (root / "runtime/sidera-input-test").exists() and re.search(
                r"Using public X11 display (:\d+)", (root / "gnome.log").read_text())

        wait_for(display_ready, "isolated GNOME Wayland and X11 displays", timeout=20)
        env["DISPLAY"] = re.search(r"Using public X11 display (:\d+)",
                                   (root / "gnome.log").read_text())[1]
        wait_for(lambda: list((root / "runtime").glob(".mutter-Xwaylandauth.*")),
                 "isolated Xwayland authentication")
        env["XAUTHORITY"] = str(next((root / "runtime").glob(".mutter-Xwaylandauth.*")))

        bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)

        def evaluate(code):
            reply = bus.call_sync(
                "org.gnome.Shell", "/org/gnome/Shell", "org.gnome.Shell", "Eval",
                GLib.Variant("(s)", (code,)), GLib.VariantType.new("(bs)"),
                Gio.DBusCallFlags.NONE, 5000, None)
            success, value = reply.unpack()
            if not success:
                raise RuntimeError(f"Isolated Shell Eval failed: {value}")
            return json.loads(value) if value else None

        wait_for(lambda: bus.call_sync(
            "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus",
            "NameHasOwner", GLib.Variant("(s)", ("org.gnome.Shell",)),
            GLib.VariantType.new("(b)"), Gio.DBusCallFlags.NONE, 5000, None).unpack()[0],
            "isolated GNOME Shell bus service")
        def shell_ready():
            try:
                return evaluate("typeof Main !== 'undefined' && Main.overview !== undefined")
            except GLib.Error:
                return False  # The bus name can appear before its object is exported.
        wait_for(shell_ready, "isolated GNOME Shell interface")
        # Allow Shell's startup animation to finish before hiding its overview.
        # This happens before any test window or pointer device is created.
        time.sleep(2)
        evaluate("Main.overview.hide(); true")
        time.sleep(0.3)
        receiver = subprocess.Popen([sys.executable, "-c", TARGET_CODE],
                                    env=env | {"GDK_BACKEND": "wayland"},
                                    stdout=receiver_log, stderr=receiver_log)
        wait_for(lambda: evaluate(
            f"global.get_window_actors().some(w => w.meta_window.get_title() === {json.dumps(TARGET_TITLE)})"),
            "native Wayland receiver window")
        sidera = subprocess.Popen([str(root / "sidera")], env=env,
                                  stdout=sidera_log, stderr=sidera_log)
        wait_for(lambda: evaluate("global.get_window_actors().length >= 2"),
                 "Sidera overlay")
        time.sleep(0.4)
        wait_for(lambda: evaluate(
            "(()=>{let a=global.stage.get_actor_at_pos(imports.gi.Clutter.PickMode.REACTIVE,640,400);"
            "return global.get_window_actors().some(w=>"
            f"w.meta_window.get_title()==={json.dumps(TARGET_TITLE)} && a && w.contains(a));}})()"),
            "native target visible through the initial cursor region")
        if os.environ.get("SIDERA_CHECK_STABLE_VISUAL") == "1":
            probe = VisualProbe(env)
        evaluate("global.sideraTestPointer = global.stage.get_context().get_backend()"
                 ".get_default_seat().create_virtual_device("
                 "imports.gi.Clutter.InputDeviceType.POINTER_DEVICE); true")

        def motion(x, y):
            # The first virtual motion can precede the seat's initial position.
            for attempt in range(3):
                evaluate("global.sideraTestPointer.notify_absolute_motion("
                         f"imports.gi.GLib.get_monotonic_time(), {x}, {y}); true")
                time.sleep(0.08)
                if evaluate("global.get_pointer()")[:2] == [x, y]:
                    return
            raise AssertionError(f"Virtual pointer did not reach {(x, y)}")

        def button(pressed):
            state = "PRESSED" if pressed else "RELEASED"
            evaluate("global.sideraTestPointer.notify_button("
                     "imports.gi.GLib.get_monotonic_time(), 1, "
                     f"imports.gi.Clutter.ButtonState.{state}); true")
            time.sleep(0.08)

        def click(x, y):
            motion(x, y)
            button(True)
            button(False)

        def counts():
            events = []
            for line in (root / "receiver.log").read_text().splitlines():
                if line.startswith("{"):
                    events.append(json.loads(line)["event"])
            return events.count("pressed"), events.count("released")

        def native_click():
            before = counts()
            click(640, 400)
            expected = before[0] + 1, before[1] + 1
            wait_for(lambda: counts() == expected,
                     f"native press/release {expected}; currently {counts()}", timeout=2)
            return counts()

        result["initial_click"] = native_click()
        for cycle in range(2):
            before = counts()
            click(1248, 264)  # Pen: (800 - 490)/2 + 109, same anchor when folded.
            time.sleep(0.3)
            motion(400, 350 + cycle * 100)
            button(True)
            for x, y in ((440, 370), (490, 390), (540, 410)):
                motion(x, y + cycle * 100)
            button(False)
            time.sleep(0.5)  # Allow an unoptimized build to finish its queued drawing.
            if counts() != before:
                raise AssertionError("Drawing leaked pointer events into the native window")
            if probe:
                wait_for(lambda: any(probe.ink()), "visible ink in the visual backing pixmap")
            before_ink = probe.ink() if probe else None
            if probe and not any(before_ink):
                raise AssertionError("The drawing did not produce visible ink pixels")
            click(1248, 224)  # Cursor button.
            time.sleep(0.4)
            if probe and probe.ink() != before_ink:
                raise AssertionError("Ink pixels changed during the cursor switch")
            if probe and cycle == 0:
                bus.call_sync(
                    "org.gnome.Shell.Screenshot", "/org/gnome/Shell/Screenshot",
                    "org.gnome.Shell.Screenshot", "Screenshot",
                    GLib.Variant("(bbs)", (False, False, str(root / "cursor.png"))),
                    None, Gio.DBusCallFlags.NONE, 5000, None)
            result["cycles"].append({"cycle": cycle + 1, "native_click": native_click(),
                                     "ink_pixels": sum(bool(p) for p in before_ink) if probe else None})
        if probe:
            motion(1270, 185)  # Sidebar background, outside its buttons.
            button(True)
            motion(1270, 230)
            button(False)
            time.sleep(0.3)
            click(1248, 309)  # Pen after moving the sidebar down by 45 pixels.
            time.sleep(0.3)
            click(1248, 269)
            time.sleep(0.4)
            result["after_sidebar_drag"] = native_click()
            result["visual_mapping_events"] = probe.mapping_events()
            if result["visual_mapping_events"]:
                raise AssertionError("The permanent visual was unmapped or recreated")
        if sidera.poll() is not None or compositor.poll() is not None:
            raise AssertionError("Sidera or its isolated compositor exited unexpectedly")
        result["passed"] = True
        print("PASS: two drawing/cursor cycles delivered native Wayland press/release pairs")
        return 0
    except Exception as error:
        result["error"] = str(error)
        if bus is not None and compositor is not None and compositor.poll() is None:
            try:
                bus.call_sync(
                    "org.gnome.Shell.Screenshot", "/org/gnome/Shell/Screenshot",
                    "org.gnome.Shell.Screenshot", "Screenshot",
                    GLib.Variant("(bbs)", (False, False, str(root / "failure.png"))),
                    None, Gio.DBusCallFlags.NONE, 5000, None)
            except GLib.Error:
                pass
        print(f"FAIL: {error}", file=sys.stderr)
        return 1
    finally:
        if probe:
            probe.close()
        for process in (sidera, receiver, compositor):
            stop(process)
        for stream in (sidera_log, receiver_log, compositor_log):
            stream.close()
        (root / "result.json").write_text(json.dumps(result, indent=2) + "\n")


def main():
    if len(sys.argv) == 3 and sys.argv[1] == "--inside":
        return inside(Path(sys.argv[2]))
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--output-dir", type=Path, help="new directory for test logs")
    parser.add_argument("--check-stable-visual", action="store_true",
                        help="check split overlay mapping, ink preservation and sidebar dragging")
    args = parser.parse_args()
    if sys.platform != "linux":
        parser.error("This test requires Linux")
    binary = args.binary.resolve()
    if not binary.is_file():
        parser.error("The built Sidera binary does not exist")
    for command in ("bwrap", "dbus-run-session", "gnome-shell", "Xwayland"):
        if shutil.which(command) is None:
            parser.error(f"Missing dependency: {command}")
    if args.output_dir:
        root = args.output_dir.resolve()
        root.mkdir(parents=True, exist_ok=False)
    else:
        root = Path(tempfile.mkdtemp(prefix="sidera-mutter-test-", dir="/var/tmp"))
    for name in ("home", "runtime"):
        (root / name).mkdir(mode=0o700)
    shutil.copy2(binary, root / "sidera")
    shutil.copy2(__file__, root / "runner.py")
    (root / "dbus.conf").write_text(
        '<busconfig><type>session</type><listen>unix:tmpdir=/tmp</listen>'
        '<auth>EXTERNAL</auth><apparmor mode="disabled"/>'
        '<policy context="default"><allow send_destination="*" eavesdrop="true"/>'
        '<allow eavesdrop="true"/><allow own="*"/></policy></busconfig>\n')
    env = {name: value for name, value in os.environ.items()
           if not name.startswith(("SIDERA_", "XDG_")) and name not in (
               "DISPLAY", "WAYLAND_DISPLAY", "WAYLAND_SOCKET", "XAUTHORITY",
               "DBUS_SESSION_BUS_ADDRESS", "SESSION_MANAGER")}
    env["SIDERA_TEST_ISOLATED"] = "1"
    if args.check_stable_visual:
        env["SIDERA_CHECK_STABLE_VISUAL"] = "1"
    print(f"Isolated test artifacts: {root}", flush=True)
    with (root / "harness.log").open("w") as output:
        result = subprocess.run(
            ["bwrap", "--die-with-parent", "--unshare-pid", "--unshare-net",
             "--ro-bind", "/", "/", "--dev", "/dev", "--proc", "/proc",
             "--tmpfs", "/tmp", "--tmpfs", "/run",
             "--bind", str(root / "home"), str(Path.home()),
             "--bind", str(root), str(root), "--chdir", str(root),
             "dbus-run-session", f"--config-file={root / 'dbus.conf'}", "--",
             sys.executable, str(root / "runner.py"), "--inside", str(root)],
            env=env, stdout=output, stderr=subprocess.STDOUT, timeout=90)
    print((root / "harness.log").read_text(), end="")
    return result.returncode


if __name__ == "__main__":
    raise SystemExit(main())
