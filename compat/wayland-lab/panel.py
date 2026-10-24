#!/usr/bin/python3
"""Visible controls and native GTK input receiver for the nested test desktop."""
import os
from pathlib import Path
import subprocess
import time
import gi
gi.require_version("Gtk", "4.0")
from gi.repository import Gtk, GLib

ROOT = Path(os.environ["SIDERA_LAB_ROOT"])
process = None
stream = None
status = None
backend = "wayland"


def stop():
    global process, stream
    if process is not None and process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()
    process = None
    if stream is not None:
        stream.close()
        stream = None


def start(mode):
    global process, stream, backend
    stop()
    backend = mode
    name = f"sidera-{mode}-{time.time_ns()}.log"
    stream = (ROOT / name).open("w")
    env = os.environ | {"SIDERA_BACKEND": mode, "SIDERA_NO_SPLASH": "1",
                        "WPS_API_DEBUG": "0", "RUST_LOG": "info"}
    if mode == "wayland":
        env.pop("DISPLAY", None)  # Native mode cannot silently fall back to X11.
    process = subprocess.Popen(["/usr/bin/sidera"], env=env, stdout=stream, stderr=stream)
    status.set_text("当前：原生 Wayland" if mode == "wayland" else "当前：XWayland 对照")


def activate(application):
    global status
    window = Gtk.ApplicationWindow(application=application, title="Sidera · Wayland 测试桌面")
    window.fullscreen()
    box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12)
    box.set_halign(Gtk.Align.CENTER)
    box.set_valign(Gtk.Align.CENTER)
    box.set_margin_top(20)
    box.set_margin_bottom(20)
    scroll = Gtk.ScrolledWindow()
    scroll.set_policy(Gtk.PolicyType.NEVER, Gtk.PolicyType.AUTOMATIC)
    scroll.set_child(box)
    window.set_child(scroll)
    def note(text):
        label = Gtk.Label(label=text)
        label.set_wrap(True)
        label.set_max_width_chars(42)
        box.append(label)
    title = Gtk.Label()
    title.set_markup('<span size="xx-large" weight="bold">Wayland 测试桌面</span>')
    box.append(title)
    note("这是独立测试窗口。结束测试后回到原来的 Ubuntu 桌面。")
    note("若看到两套工具栏，请先退出原桌面的 Sidera。")
    status = Gtk.Label(label="正在启动…")
    box.append(status)
    row = Gtk.Box(spacing=12)
    for label, mode in (("原生 Wayland", "wayland"), ("XWayland 对照", "x11")):
        button = Gtk.Button(label=label)
        button.connect("clicked", lambda _, mode=mode: start(mode))
        row.append(button)
    pause = Gtk.Button(label="关闭 Sidera")
    pause.connect("clicked", lambda _: (stop(), status.set_text("Sidera 已关闭")))
    row.append(pause)
    box.append(row)
    counts = [0, 0]
    # A GtkButton's own gesture can claim the sequence before this observer gets
    # release. Use a passive frame so this is a faithful press/release receiver.
    target = Gtk.Frame()
    counter = Gtk.Label(label="穿透测试：按下 0 次 / 松开 0 次")
    target.set_child(counter)
    target.set_size_request(440, 90)
    gesture = Gtk.GestureClick()
    def count(_, number, x, y, index):
        counts[index] += 1
        counter.set_text(f"穿透测试：按下 {counts[0]} 次 / 松开 {counts[1]} 次")
    gesture.connect("pressed", lambda *args: count(*args, 0))
    gesture.connect("released", lambda *args: count(*args, 1))
    target.add_controller(gesture)
    box.append(target)
    note("先选画笔画线，再点箭头，最后点击上面的穿透测试按钮。")
    scales = Gtk.Box(spacing=10)
    scales.append(Gtk.Label(label="缩放："))
    for scale in (1, 1.25, 1.5, 2):
        button = Gtk.Button(label=f"{scale * 100:g}%")
        button.connect("clicked", lambda _, scale=scale: subprocess.run(
            ["swaymsg", "output", "*", "scale", str(scale)], timeout=5))
        scales.append(button)
    box.append(scales)
    note("缩放会按比例保留笔迹并重置撤销；测试内容不带回正常桌面。")
    close = Gtk.Button(label="结束测试，返回 Ubuntu")
    close.connect("clicked", lambda _: subprocess.run(["swaymsg", "exit"], timeout=5))
    box.append(close)
    window.present()
    start("wayland")
    def health():
        if process is not None and process.poll() is not None:
            status.set_text(f"Sidera 已退出（{process.returncode}）；可点击上方按钮重新启动。")
        return True
    GLib.timeout_add(1000, health)


app = Gtk.Application(application_id="org.sidera.WaylandLab")
app.connect("activate", activate)
try:
    app.run(None)
finally:
    stop()
