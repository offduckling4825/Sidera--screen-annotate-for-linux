// ============================================================
// Sidera - 程序入口（纯 Rust / X11，无 Qt）
// Copyright (C) 2026 Carl_Jin   GNU GPL v3
// ============================================================
mod app;
mod gesture;
mod paint;
mod splash;
mod text;
mod ui;
mod wps;
mod x11;

use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use tiny_skia::{IntSize, Pixmap};
use x11rb::connection::Connection;
use x11rb::protocol::xinput::{self, XIEventMask};
use x11rb::protocol::xproto::{ConnectionExt, KeyButMask, Rectangle, Window};
use x11rb::protocol::Event;

use app::{now_ms, App};
use gesture::TouchKind;
use text::Fonts;
use ui::{Btn, MoreHit, PopupHit, R, SetHit};
use x11::X11;
use wps::{WpsBridge, WpsEvent};

struct Ip {
    dragging: bool,
    drag_start_y: i32,
    drag_mouse_y0: i32,
    moved: bool,
    collapse_press: bool,
    // 触摸：正在按下的触点 id，以及每个触点是否起始于 UI（点按钮而非画线）
    touch_ids: HashSet<i32>,
    touch_over_ui: HashMap<i32, bool>,
}

struct Rt {
    x11: X11,
    win: Window,
    gc: x11rb::protocol::xproto::Gcontext,
    fonts: Fonts,
    icon: Option<Pixmap>,
    icon_big: Option<Pixmap>,
}

impl Rt {
    fn redraw(&self, app: &App, r: R) {
        let x = r.x.max(0);
        let y = r.y.max(0);
        let x2 = (r.x + r.w).min(app.screen_w);
        let y2 = (r.y + r.h).min(app.screen_h);
        if x2 <= x || y2 <= y {
            return;
        }
        if let Some(pm) = ui::render_region(app, &self.fonts, self.icon.as_ref(), x, y, x2 - x, y2 - y)
        {
            let _ = self.x11.put_pixmap(self.win, self.gc, &pm, x, y);
        }
    }
    fn redraw_full(&self, app: &App) {
        self.redraw(
            app,
            R {
                x: 0,
                y: 0,
                w: app.screen_w,
                h: app.screen_h,
            },
        );
    }
}

fn rect_of(r: R, pad: i32) -> Rectangle {
    let x = (r.x - pad).max(0);
    let y = (r.y - pad).max(0);
    let w = (r.w + pad * 2).min(i16::MAX as i32);
    let h = (r.h + pad * 2).min(i16::MAX as i32);
    Rectangle {
        x: x as i16,
        y: y as i16,
        width: w as u16,
        height: h as u16,
    }
}

fn apply_input_shape(rt: &Rt, app: &App) {
    if app.settings_open {
        let rects = [rect_of(ui::settings_rect(app), 0)];
        let _ = rt.x11.set_input_shape(rt.win, &rects, false);
        return;
    }
    if app.whiteboard {
        let _ = rt.x11.set_input_shape(rt.win, &[], true);
        return;
    }
    if app.mode == 0 {
        let mut rects = Vec::new();
        for right in [false, true] {
            rects.push(rect_of(ui::sidebar_rect(app, right), 4));
        }
        if app.pen_popup_visible {
            rects.push(rect_of(ui::pen_popup_rect(app), 4));
        }
        if app.eraser_popup_visible {
            rects.push(rect_of(ui::eraser_popup_rect(app), 4));
        }
        if app.more_menu_open {
            rects.push(rect_of(ui::more_menu_rect(app), 4));
        }
        let _ = rt.x11.set_input_shape(rt.win, &rects, false);
    } else {
        let _ = rt.x11.set_input_shape(rt.win, &[], true);
    }
}

fn close_all_popups(app: &mut App) {
    app.pen_popup_visible = false;
    app.eraser_popup_visible = false;
    app.more_menu_open = false;
}

/// 当前点是否落在任何 UI 元素上（侧栏/弹窗/更多菜单/设置面板）。
/// 画笔/橡皮进入这些区域应中断，避免把墨迹画到侧栏底下（半透明侧栏会透出来）。
fn over_ui(app: &App, x: i32, y: i32) -> bool {
    for right in [false, true] {
        if ui::sidebar_rect(app, right).contains(x, y) {
            return true;
        }
    }
    if app.pen_popup_visible && ui::pen_popup_rect(app).contains(x, y) {
        return true;
    }
    if app.eraser_popup_visible && ui::eraser_popup_rect(app).contains(x, y) {
        return true;
    }
    if app.more_menu_open && ui::more_menu_rect(app).contains(x, y) {
        return true;
    }
    if app.settings_open && ui::settings_rect(app).contains(x, y) {
        return true;
    }
    false
}

fn clamp_sb(app: &mut App) {
    let h = if app.collapsed {
        app.collapsed_h()
    } else {
        app.sb_height()
    };
    app.sb_y = app.sb_y.clamp(0, (app.screen_h - h).max(0));
}

fn switch_to_cursor(rt: &Rt, app: &mut App) {
    close_all_popups(app);
    app.mode = 0;
    app.is_drawing = false;
    app.reset_palm_gesture();
    apply_input_shape(rt, app);
    rt.redraw_full(app);
    log::info!("[INFO] 光标模式");
}

fn switch_to_draw(rt: &Rt, app: &mut App, mode: i32) {
    close_all_popups(app);
    app.mode = mode;
    if app.collapsed {
        expand_sidebars(rt, app);
    }
    app.is_drawing = false;
    app.reset_palm_gesture();
    apply_input_shape(rt, app);
    rt.redraw_full(app);
    log::info!(
        "[INFO] 绘画模式: {}",
        if mode == 1 { "画笔" } else { "橡皮擦" }
    );
}

fn collapse_sidebars(rt: &Rt, app: &mut App) {
    if app.collapsed {
        return;
    }
    app.collapsed = true;
    if app.mode != 0 {
        switch_to_cursor(rt, app);
    }
    clamp_sb(app);
    app.collapse_ts = now_ms();
    apply_input_shape(rt, app);
    rt.redraw_full(app);
    log::info!("[INFO] 侧边栏已收缩");
}

fn expand_sidebars(rt: &Rt, app: &mut App) {
    if !app.collapsed {
        return;
    }
    app.collapsed = false;
    clamp_sb(app);
    apply_input_shape(rt, app);
    rt.redraw_full(app);
    log::info!("[INFO] 侧边栏已展开");
}

fn toggle_collapse(rt: &Rt, app: &mut App) {
    if app.collapsed {
        expand_sidebars(rt, app);
    } else {
        collapse_sidebars(rt, app);
    }
}

fn go_prev(rt: &Rt, app: &mut App, wps: Option<&WpsBridge>) {
    if app.whiteboard {
        app.save_current_page();
        if app.current_slide > 1 {
            app.current_slide -= 1;
        }
        app.load_page(app.current_slide);
        rt.redraw_full(app);
        return;
    }
    if app.wps_debug && app.wps_connected {
        if let Some(w) = wps {
            w.enqueue("PREV");
        }
        return;
    }
    app.save_current_page();
    if app.current_slide > 1 {
        app.current_slide -= 1;
    }
    rt.x11.fake_key(rt.x11.key_up);
    app.load_page(app.current_slide);
    rt.redraw_full(app);
}

fn go_next(rt: &Rt, app: &mut App, wps: Option<&WpsBridge>) {
    if app.whiteboard {
        app.save_current_page();
        let mut p = app.current_slide + 1;
        if p > app::CACHE_BOARD {
            p = 1;
            app.wb_limit_msg_until = now_ms() + 3000;
        }
        app.current_slide = p;
        app.load_page(app.current_slide);
        rt.redraw_full(app);
        return;
    }
    if app.wps_debug && app.wps_connected {
        if let Some(w) = wps {
            w.enqueue("NEXT");
        }
        return;
    }
    app.save_current_page();
    app.current_slide += 1;
    rt.x11.fake_key(rt.x11.key_down);
    app.load_page(app.current_slide);
    rt.redraw_full(app);
}

fn exit_presentation(rt: &Rt, app: &mut App) {
    if app.whiteboard {
        app.whiteboard_bg_index = (app.whiteboard_bg_index + 1) % app::WHITEBOARD_COLORS.len() as i32;
        rt.redraw_full(app);
        return;
    }
    if app.mode != 0 {
        switch_to_cursor(rt, app);
    }
    rt.x11.fake_key(rt.x11.key_escape);
    log::info!("[INFO] 退出放映（已回光标模式并发送 ESC）");
}

fn do_screenshot(x11: &X11) {
    let Some((rgba, w, h)) = x11.screenshot() else {
        log::warn!("[WARN] 截图失败");
        return;
    };
    let Some(img) = image::RgbaImage::from_raw(w, h, rgba) else {
        return;
    };
    let dir = dirs::picture_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    let _ = std::fs::create_dir_all(&dir);
    let fn_ = dir.join(format!("sidera_{}.png", now_ms()));
    if img.save(&fn_).is_ok() {
        log::info!("[INFO] 截图已保存: {}", fn_.display());
    } else {
        log::warn!("[WARN] 截图保存失败: {}", fn_.display());
    }
}

fn collect_system_info(rt: &Rt, app: &App) -> String {
    let mut s = String::new();
    s.push_str("=== 系统环境诊断 ===\n");
    if let Ok(os) = std::fs::read_to_string("/etc/os-release") {
        for line in os.lines() {
            if let Some(v) = line.strip_prefix("PRETTY_NAME=") {
                s.push_str(&format!("系统: {}\n", v));
            }
        }
    }
    s.push_str(&format!("架构: {}\n", std::env::consts::ARCH));
    s.push_str(&format!(
        "XDG_SESSION_TYPE: {}\n",
        std::env::var("XDG_SESSION_TYPE").unwrap_or_default()
    ));
    s.push_str(&format!(
        "DISPLAY: {}\n",
        std::env::var("DISPLAY").unwrap_or_default()
    ));
    s.push_str(&format!(
        "合成器: {}\n",
        if rt.x11.has_compositor() { "有" } else { "无" }
    ));
    s.push_str(&format!(
        "XShape: {}\n",
        if rt.x11.shape_available() {
            "扩展可用"
        } else {
            "扩展不可用!"
        }
    ));
    s.push_str(&format!(
        "屏幕: {}x{}\n",
        rt.x11.width, rt.x11.height
    ));
    s.push_str(&format!(
        "模式: {}\n",
        match app.mode {
            0 => "光标",
            1 => "画笔",
            _ => "橡皮",
        }
    ));
    s.push_str(&format!("WPS 全屏: {}\n", app.wps_fullscreen));
    s.push_str(&format!("WPS 已连接: {}\n", app.wps_connected));
    s
}

fn open_settings(rt: &Rt, app: &mut App) {
    if std::env::var("SIDERA_TRACE").is_ok() { log::info!("[SETTINGS] 打开"); }
    app.settings_open = true;
    close_all_popups(app);
    apply_input_shape(rt, app);
    rt.redraw_full(app);
}

fn handle_settings_hit(rt: &Rt, app: &mut App, hit: SetHit) {
    if std::env::var("SIDERA_TRACE").is_ok() { log::info!("[SETTINGS] 命中 {:?}", hit); }
    match hit {
        SetHit::AlphaDec => {
            app.sidebar_alpha = (app.sidebar_alpha as i32 - 20).clamp(30, 255) as u8;
            app::save_settings(app);
        }
        SetHit::AlphaInc => {
            app.sidebar_alpha = (app.sidebar_alpha as i32 + 20).clamp(30, 255) as u8;
            app::save_settings(app);
        }
        SetHit::ScaleDec => {
            app.sb_scale = (app.sb_scale - 0.1).clamp(0.6, 1.4);
            clamp_sb(app);
            app::save_settings(app);
        }
        SetHit::ScaleInc => {
            app.sb_scale = (app.sb_scale + 0.1).clamp(0.6, 1.4);
            clamp_sb(app);
            app::save_settings(app);
        }
        SetHit::Autostart => ui::set_autostart(!ui::is_autostart()),
        SetHit::WpsDebug => {
            app.wps_debug = !app.wps_debug;
            app::save_settings(app);
        }
        SetHit::RightClick => {
            app.right_click_cursor_on = !app.right_click_cursor_on;
            app::save_settings(app);
        }
        SetHit::EraseTrig => {
            app.erase_by_finger = !app.erase_by_finger;
            app::save_settings(app);
        }
        SetHit::ThrDec => {
            app.large_touch_threshold = (app.large_touch_threshold - 10.0).clamp(20.0, 260.0);
            app::save_settings(app);
        }
        SetHit::ThrInc => {
            app.large_touch_threshold = (app.large_touch_threshold + 10.0).clamp(20.0, 260.0);
            app::save_settings(app);
        }
        SetHit::EraseScaleDec => {
            app.large_erase_scale10 = (app.large_erase_scale10 - 1).clamp(8, 25);
            app::save_settings(app);
        }
        SetHit::EraseScaleInc => {
            app.large_erase_scale10 = (app.large_erase_scale10 + 1).clamp(8, 25);
            app::save_settings(app);
        }
        SetHit::ClearLog => {
            let _ = std::fs::remove_file(wps::log_file());
            app.log_cleared_until = now_ms() + 1200;
        }
        SetHit::SysInfo => {
            let info = collect_system_info(rt, app);
            log::info!("\n{}", info);
            let _ = std::fs::write("/tmp/sidera-diag.txt", &info);
        }
        SetHit::Quit => std::process::exit(0),
        SetHit::Done => {
            app.settings_open = false;
            apply_input_shape(rt, app);
        }
    }
    rt.redraw_full(app);
}

fn first_hit(r: R, x: i32, y: i32) -> bool {
    r.contains(x, y)
}

fn on_press(
    rt: &Rt,
    app: &mut App,
    ip: &mut Ip,
    wps: Option<&WpsBridge>,
    x: i32,
    y: i32,
    button: u8,
) {
    if app.settings_open {
        if let Some(hit) = ui::settings_hit(app, x, y) {
            handle_settings_hit(rt, app, hit);
        }
        return;
    }
    if app.more_menu_open {
        if let Some(hit) = ui::more_menu_hit(app, x, y) {
            app.more_menu_open = false;
            match hit {
                MoreHit::Screenshot => do_screenshot(&rt.x11),
                MoreHit::Settings => open_settings(rt, app),
            }
        } else {
            app.more_menu_open = false;
            rt.redraw_full(app);
        }
        return;
    }
    if app.pen_popup_visible {
        if let Some(hit) = ui::pen_popup_hit(app, x, y) {
            match hit {
                PopupHit::Color(i) => app.cur_color = i,
                PopupHit::PenSize(i) => app.cur_pen = i,
                _ => {}
            }
            rt.redraw_full(app);
            return;
        }
    }
    if app.eraser_popup_visible {
        if let Some(hit) = ui::eraser_popup_hit(app, x, y) {
            match hit {
                PopupHit::EraserSize(i) => app.cur_eraser = i,
                PopupHit::ClearAll => app.clear_canvas(),
                _ => {}
            }
            rt.redraw_full(app);
            return;
        }
    }

    // 侧边栏按钮
    for right in [false, true] {
        for (k, r) in ui::sidebar_buttons(app, right) {
            if first_hit(r, x, y) {
                app.popup_on_right = right;
                handle_btn(rt, app, wps, k, right);
                return;
            }
        }
        let sb = ui::sidebar_rect(app, right);
        if sb.contains(x, y) {
            ip.dragging = true;
            ip.moved = false;
            ip.drag_start_y = app.sb_y;
            ip.drag_mouse_y0 = y;
            ip.collapse_press = app.collapsed;
            return;
        }
    }

    // 画布区域
    if app.mode != 0 {
        if button == 3 {
            if app.right_click_cursor_on && app.mode == 1 {
                rt.x11.virtual_right_click(x, y);
                switch_to_cursor(rt, app);
            }
            return;
        }
        // 落在弹窗/菜单等 UI 上：不落笔
        if over_ui(app, x, y) {
            return;
        }
        close_all_popups(app);
        app.push_undo();
        app.is_drawing = true;
        app.last_pt = (x, y);
        app.stroke_start_ms = now_ms();
        app.last_pen_w = app.timed_pen_width();
        let w;
        if app.mode == 2 {
            w = app.eraser_width();
            paint::stroke_segment(app, (x, y), (x, y), true, w);
        } else {
            w = app.last_pen_w;
            paint::stroke_tapered(app, (x, y), (x, y), w, w);
        }
        let _ = button;
        rt.redraw(
            app,
            R {
                x: x - w,
                y: y - w,
                w: w * 2,
                h: w * 2,
            },
        );
    }
}

fn handle_btn(rt: &Rt, app: &mut App, wps: Option<&WpsBridge>, k: Btn, right: bool) {
    match k {
        Btn::Collapse => toggle_collapse(rt, app),
        Btn::Cursor => switch_to_cursor(rt, app),
        Btn::Pen => {
            if app.mode == 1 {
                let was = app.pen_popup_visible;
                close_all_popups(app);
                app.pen_popup_visible = !was;
            } else {
                switch_to_draw(rt, app, 1);
            }
            apply_input_shape(rt, app);
            rt.redraw_full(app);
        }
        Btn::Eraser => {
            if app.mode == 2 {
                let was = app.eraser_popup_visible;
                close_all_popups(app);
                app.eraser_popup_visible = !was;
            } else {
                switch_to_draw(rt, app, 2);
            }
            apply_input_shape(rt, app);
            rt.redraw_full(app);
        }
        Btn::Clear => {
            app.clear_current_strokes();
            switch_to_cursor(rt, app);
        }
        Btn::Undo => {
            app.undo_last();
            rt.redraw_full(app);
        }
        Btn::Prev => go_prev(rt, app, wps),
        Btn::Next => go_next(rt, app, wps),
        Btn::Exit => exit_presentation(rt, app),
        Btn::Whiteboard => {
            app.toggle_whiteboard();
            apply_input_shape(rt, app);
            rt.redraw_full(app);
        }
        Btn::More => {
            close_all_popups(app);
            app.more_menu_open = true;
            apply_input_shape(rt, app);
            rt.redraw_full(app);
        }
    }
    let _ = right;
}

fn on_motion(rt: &Rt, app: &mut App, ip: &mut Ip, x: i32, y: i32) {
    if ip.dragging {
        let dy = y - ip.drag_mouse_y0;
        if !ip.moved && dy.abs() <= 4 {
            return;
        }
        ip.moved = true;
        let h = if app.collapsed {
            app.collapsed_h()
        } else {
            app.sb_height()
        };
        app.sb_y = (ip.drag_start_y + dy).clamp(0, (app.screen_h - h).max(0));
        apply_input_shape(rt, app);
        rt.redraw_full(app);
        return;
    }
    if !app.is_drawing || app.mode == 0 {
        return;
    }
    // 笔尖进入 UI 区域：中断本次笔画（防止墨迹落到半透明侧栏底下）
    if over_ui(app, x, y) {
        app.is_drawing = false;
        app.reset_palm_gesture();
        return;
    }
    let cur = (x, y);
    let prev = app.last_pt;
    let erase = app.mode == 2;
    let w = if erase {
        app.eraser_width()
    } else {
        app.timed_pen_width()
    };
    if erase {
        paint::stroke_segment(app, prev, cur, true, w);
    } else {
        paint::stroke_tapered(app, prev, cur, app.last_pen_w, w);
    }
    app.last_pen_w = w;
    app.last_pt = cur;
    let x1 = prev.0.min(cur.0) - w;
    let y1 = prev.1.min(cur.1) - w;
    let x2 = prev.0.max(cur.0) + w;
    let y2 = prev.1.max(cur.1) + w;
    rt.redraw(
        app,
        R {
            x: x1,
            y: y1,
            w: x2 - x1,
            h: y2 - y1,
        },
    );
}

fn on_release(rt: &Rt, app: &mut App, ip: &mut Ip, _x: i32, _y: i32) {
    if app.is_drawing {
        app.is_drawing = false;
        let w = if app.mode == 2 {
            app.eraser_width()
        } else {
            app.pen_width()
        };
        let (px, py) = app.last_pt;
        rt.redraw(
            app,
            R {
                x: px - w,
                y: py - w,
                w: w * 2,
                h: w * 2,
            },
        );
    }
    if ip.dragging {
        ip.dragging = false;
        if ip.collapse_press && !ip.moved {
            toggle_collapse(rt, app);
        }
    }
}

fn handle_event(rt: &Rt, app: &mut App, ip: &mut Ip, wps: Option<&WpsBridge>, ev: Event) {
    // 有触摸按下时，屏蔽合成出来的鼠标事件，避免一次触摸被处理两遍
    if !ip.touch_ids.is_empty() {
        if let Event::ButtonPress(_) | Event::ButtonRelease(_) | Event::MotionNotify(_) = &ev {
            return;
        }
    }
    match ev {
        Event::Expose(e) => {
            if e.count == 0 {
                rt.redraw_full(app);
            } else {
                let _ = e.width;
            }
        }
        Event::ButtonPress(e) => {
            if std::env::var("SIDERA_TRACE").is_ok() {
                log::info!("[BTN] at ({},{}) detail={}", e.event_x, e.event_y, e.detail);
            }
            on_press(rt, app, ip, wps, e.event_x as i32, e.event_y as i32, e.detail)
        }
        Event::MotionNotify(e) => {
            if std::env::var("SIDERA_TRACE").is_ok() {
                log::info!("[MOTION] ({},{})", e.event_x, e.event_y);
            }
            on_motion(rt, app, ip, e.event_x as i32, e.event_y as i32)
        }
        Event::ButtonRelease(e) => on_release(rt, app, ip, e.event_x as i32, e.event_y as i32),
        Event::KeyPress(e) => {
            let mods = KeyButMask::CONTROL | KeyButMask::SHIFT;
            if e.detail == rt.x11.key_d && (e.state & mods) == mods {
                if app.mode != 0 {
                    switch_to_cursor(rt, app);
                } else {
                    switch_to_draw(rt, app, 1);
                }
            }
        }
        Event::XinputTouchBegin(e) => {
            let x = (e.event_x as f64 / 65536.0).round() as i32;
            let y = (e.event_y as f64 / 65536.0).round() as i32;
            let id = e.detail as i32;
            ip.touch_ids.insert(id);
            if over_ui(app, x, y) {
                // 触摸点在 UI 上：当作一次左键点击（按钮/设置/菜单），不画线
                ip.touch_over_ui.insert(id, true);
                on_press(rt, app, ip, wps, x, y, 1);
                ip.dragging = false; // 触摸不做侧栏拖动
            } else {
                ip.touch_over_ui.insert(id, false);
                if let Some(_r) = gesture::touch_event(app, id, x, y, TouchKind::Begin, 0.0) {
                    rt.redraw_full(app);
                }
            }
        }
        Event::XinputTouchUpdate(e) => {
            let id = e.detail as i32;
            if ip.touch_over_ui.get(&id).copied().unwrap_or(false) {
                return;
            }
            let x = (e.event_x as f64 / 65536.0).round() as i32;
            let y = (e.event_y as f64 / 65536.0).round() as i32;
            if over_ui(app, x, y) {
                app.reset_palm_gesture();
                return;
            }
            if let Some(r) = gesture::touch_event(app, id, x, y, TouchKind::Update, 0.0) {
                rt.redraw(app, r);
            }
        }
        Event::XinputTouchEnd(e) => {
            let id = e.detail as i32;
            ip.touch_ids.remove(&id);
            let over = ip.touch_over_ui.remove(&id).unwrap_or(false);
            if over {
                return;
            }
            let x = (e.event_x as f64 / 65536.0).round() as i32;
            let y = (e.event_y as f64 / 65536.0).round() as i32;
            if let Some(r) = gesture::touch_event(app, id, x, y, TouchKind::End, 0.0) {
                rt.redraw(app, r);
            }
        }
        _ => {}
    }
}

fn load_icon(size: u32) -> Option<Pixmap> {
    let mut cands: Vec<PathBuf> = Vec::new();
    if let Ok(env) = std::env::var("SIDERA_ICON") {
        cands.push(PathBuf::from(env));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            cands.push(dir.join("sidera.png"));
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        cands.push(cwd.join("sidera.png"));
    }
    cands.push(PathBuf::from("/usr/share/sidera/sidera.png"));
    cands.push(PathBuf::from(
        "/usr/share/icons/hicolor/256x256/apps/sidera.png",
    ));
    for p in cands {
        let Ok(img) = image::open(&p) else { continue };
        let rgba = image::imageops::resize(
            &img.to_rgba8(),
            size,
            size,
            image::imageops::FilterType::Lanczos3,
        );
        let mut data = vec![0u8; (size * size * 4) as usize];
        for (i, px) in rgba.pixels().enumerate() {
            let a = px[3] as u16;
            let mul = |c: u8| ((c as u16 * a + 127) / 255) as u8;
            data[i * 4] = mul(px[0]);
            data[i * 4 + 1] = mul(px[1]);
            data[i * 4 + 2] = mul(px[2]);
            data[i * 4 + 3] = px[3];
        }
        if let Some(pm) = Pixmap::from_vec(data, IntSize::from_wh(size, size)?) {
            return Some(pm);
        }
    }
    None
}

fn single_instance() -> Option<UnixListener> {
    let path = dirs::runtime_dir()
        .map(|d| d.join("sidera.sock"))
        .unwrap_or_else(|| {
            let user = std::env::var("USER").unwrap_or_else(|_| "user".to_string());
            PathBuf::from(format!("/tmp/sidera-{}.sock", user))
        });
    if let Ok(mut s) = UnixStream::connect(&path) {
        let _ = s.write_all(b"show");
        let _ = s.flush();
        log::warn!("[WARN] Sidera 已在运行，退出新实例");
        return None;
    }
    let _ = std::fs::remove_file(&path);
    match UnixListener::bind(&path) {
        Ok(l) => {
            let _ = l.set_nonblocking(true);
            Some(l)
        }
        Err(_) => None,
    }
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // 软件渲染（无 GPU 老机器）
    std::env::set_var("LIBGL_ALWAYS_SOFTWARE", "1");

    if std::env::var("XDG_SESSION_TYPE")
        .map(|v| v.to_lowercase() == "wayland")
        .unwrap_or(false)
        || std::env::var("WAYLAND_DISPLAY").is_ok()
    {
        log::warn!("[WARN] 检测到 Wayland 会话；当前 Rust 版仅实现 X11 后端，请切换到 X11 会话");
    }

    let single = single_instance();
    if single.is_none() {
        return;
    }

    let x11 = match X11::connect() {
        Ok(x) => x,
        Err(e) => {
            log::error!("[ERROR] 无法连接 X11: {}", e);
            return;
        }
    };
    log::info!("[INFO] 平台: x11  屏幕 {}x{}", x11.width, x11.height);

    let fonts = Fonts::load();
    let icon_big = load_icon(84);
    let icon = load_icon(44);

    let (win, gc) = match x11.create_overlay() {
        Ok(v) => v,
        Err(e) => {
            log::error!("[ERROR] 创建覆盖层失败: {}", e);
            return;
        }
    };

    // XInput2 触摸事件（若可用）
    {
        let mask = XIEventMask::TOUCH_BEGIN | XIEventMask::TOUCH_UPDATE | XIEventMask::TOUCH_END;
        let em = xinput::EventMask {
            deviceid: 0,
            mask: vec![mask],
        };
        if let Err(e) = xinput::xi_select_events(&x11.conn, win, &[em]) {
            log::warn!("[WARN] XInput2 触摸选择失败（触摸可能不可用）: {}", e);
        }
    }

    let hotkey_ok = x11.grab_hotkey();

    let mut canvas = Pixmap::new(x11.width as u32, x11.height as u32).unwrap();
    canvas.fill(tiny_skia::Color::TRANSPARENT);
    let mut app = App::new(canvas, x11.width, x11.height);
    app.hotkey_ok = hotkey_ok;
    app::load_settings(&mut app);
    // 环境变量仅本次运行覆盖（不落盘），与 C++ 版一致
    if let Ok(v) = std::env::var("WPS_API_DEBUG") {
        app.wps_debug = v == "1";
    }
    clamp_sb(&mut app);

    let rt = Rt {
        x11,
        win,
        gc,
        fonts,
        icon,
        icon_big,
    };

    if !rt.x11.has_compositor() {
        log::warn!(
            "[WARN] 未检测到 X11 合成器！透明画布将无法显示。请开启合成器，例如: picom & 或 xcompmgr &"
        );
    }

    // 冒烟测试：渲染一帧后立即退出（用于无显示环境自检）
    if std::env::var("SIDERA_SMOKE").is_ok() {
        log::info!("[SMOKE] 渲染一帧后退出");
        rt.redraw_full(&app);
        std::thread::sleep(Duration::from_millis(200));
        rt.x11.destroy_window(win);
        return;
    }

    // 调试：画一条穿过侧边栏的笔迹并保存整屏 PNG，用于检查渲染顺序
    if let Ok(path) = std::env::var("SIDERA_DEBUG_DRAW") {
        app.mode = 1;
        paint::stroke_tapered(&mut app, (0, 800), (400, 800), 6, 6);
        paint::stroke_tapered(&mut app, (30, 620), (30, 900), 6, 6);
        paint::stroke_tapered(&mut app, (2500, 800), (2560, 800), 6, 6);
        if let Some(pm) =
            ui::render_region(&app, &rt.fonts, rt.icon.as_ref(), 0, 0, app.screen_w, app.screen_h)
        {
            let mut rgba = vec![0u8; pm.data().len()];
            for (i, px) in pm.data().chunks_exact(4).enumerate() {
                let a = px[3] as u32;
                let un = |c: u8| if a == 0 { 0 } else { ((c as u32 * 255 + a / 2) / a).min(255) as u8 };
                rgba[i * 4] = un(px[0]);
                rgba[i * 4 + 1] = un(px[1]);
                rgba[i * 4 + 2] = un(px[2]);
                rgba[i * 4 + 3] = px[3];
            }
            if let Some(img) = image::RgbaImage::from_raw(pm.width(), pm.height(), rgba) {
                let _ = img.save(&path);
                log::info!("[DEBUG] 已保存渲染结果: {}", path);
            }
        }
        rt.x11.destroy_window(win);
        return;
    }

    // 调试：直接打开设置面板并渲染保存
    if let Ok(path) = std::env::var("SIDERA_DEBUG_SETTINGS") {
        app.settings_open = true;
        if let Some(pm) =
            ui::render_region(&app, &rt.fonts, rt.icon.as_ref(), 0, 0, app.screen_w, app.screen_h)
        {
            let mut rgba = vec![0u8; pm.data().len()];
            for (i, px) in pm.data().chunks_exact(4).enumerate() {
                let a = px[3] as u32;
                let un = |c: u8| if a == 0 { 0 } else { ((c as u32 * 255 + a / 2) / a).min(255) as u8 };
                rgba[i * 4] = un(px[0]);
                rgba[i * 4 + 1] = un(px[1]);
                rgba[i * 4 + 2] = un(px[2]);
                rgba[i * 4 + 3] = px[3];
            }
            if let Some(img) = image::RgbaImage::from_raw(pm.width(), pm.height(), rgba) {
                let _ = img.save(&path);
                log::info!("[DEBUG] 设置面板已保存: {}", path);
            }
        }
        rt.x11.destroy_window(win);
        return;
    }

    // 调试：查询窗口的 XShape 输入区域
    if std::env::var("SIDERA_DEBUG_SHAPE").is_ok() {
        use x11rb::protocol::shape;
        apply_input_shape(&rt, &app);
        std::thread::sleep(Duration::from_millis(200));
        match shape::get_rectangles(&rt.x11.conn, win, shape::SK::INPUT)
            .ok()
            .and_then(|c| c.reply().ok())
        {
            Some(r) => {
                let coords: Vec<(i16, i16, u16, u16)> = r
                    .rectangles
                    .iter()
                    .map(|x| (x.x, x.y, x.width, x.height))
                    .collect();
                log::info!("[SHAPE] 输入区域矩形 {} 个: {:?}", r.rectangles.len(), coords);
            }
            None => log::info!("[SHAPE] 查询失败"),
        }
        // 再切到绘画模式（全窗输入），查询是否恢复整窗
        app.mode = 1;
        apply_input_shape(&rt, &app);
        std::thread::sleep(Duration::from_millis(200));
        if let Some(r) = shape::get_rectangles(&rt.x11.conn, win, shape::SK::INPUT)
            .ok()
            .and_then(|c| c.reply().ok())
        {
            let coords: Vec<(i16, i16, u16, u16)> = r
                .rectangles
                .iter()
                .map(|x| (x.x, x.y, x.width, x.height))
                .collect();
            log::info!("[SHAPE] 绘画模式输入区域 {} 个: {:?}", r.rectangles.len(), coords);
        }
        rt.x11.destroy_window(win);
        return;
    }

    // 调试：验证 XTest 指针移动是否生效（判断 XWayland 下能否注入事件）
    if std::env::var("SIDERA_TEST_XTEST").is_ok() {
        use x11rb::protocol::xproto;
        let before = xproto::query_pointer(&rt.x11.conn, rt.x11.root)
            .ok()
            .and_then(|c| c.reply().ok());
        rt.x11
            .virtual_right_click(123, 456); // 内部会先 fake motion
        std::thread::sleep(Duration::from_millis(300));
        let after = xproto::query_pointer(&rt.x11.conn, rt.x11.root)
            .ok()
            .and_then(|c| c.reply().ok());
        if let (Some(b), Some(a)) = (before, after) {
            log::info!(
                "[XTEST] pointer before=({},{}) after=({},{})",
                b.root_x,
                b.root_y,
                a.root_x,
                a.root_y
            );
        }
        rt.x11.destroy_window(win);
        return;
    }

    // 启动闪屏
    if std::env::var("SIDERA_NO_SPLASH").is_err() {
        splash::show(&rt.x11, &rt.fonts, rt.icon_big.as_ref(), 2000);
    }

    // 初始光标模式 + 侧边栏输入区域
    apply_input_shape(&rt, &app);
    rt.redraw_full(&app);

    // WPS 桥（若默认开启）
    let mut wps_bridge: Option<WpsBridge> = None;
    if app.wps_debug {
        wps_bridge = WpsBridge::start();
        if wps_bridge.is_none() {
            log::warn!("[WARN] WPS HTTP 16666 绑定失败");
        }
    }

    let mut ip = Ip {
        dragging: false,
        drag_start_y: 0,
        drag_mouse_y0: 0,
        moved: false,
        collapse_press: false,
        touch_ids: HashSet::new(),
        touch_over_ui: HashMap::new(),
    };

    let mut last_wps_check = Instant::now() - Duration::from_secs(10);
    let mut last_ping = Instant::now();

    loop {
        // 1. X 事件
        loop {
            match rt.x11.conn.poll_for_event() {
                Ok(Some(ev)) => handle_event(&rt, &mut app, &mut ip, wps_bridge.as_ref(), ev),
                Ok(None) => break,
                Err(e) => {
                    log::error!("[ERROR] X11 连接错误: {}", e);
                    return;
                }
            }
        }

        // 2. 单实例唤醒
        if let Some(l) = single.as_ref() {
            if let Ok((mut c, _)) = l.accept() {
                let mut buf = [0u8; 16];
                let _ = c.read(&mut buf);
                let _ = rt.x11.conn.configure_window(
                    win,
                    &x11rb::protocol::xproto::ConfigureWindowAux::new()
                        .stack_mode(x11rb::protocol::xproto::StackMode::ABOVE),
                );
                let _ = rt.x11.conn.map_window(win);
                let _ = rt.x11.conn.flush();
                rt.redraw_full(&app);
            }
        }

        // 3. WPS 桥存在性与设置同步
        if app.wps_debug && wps_bridge.is_none() {
            wps_bridge = WpsBridge::start();
        } else if !app.wps_debug && wps_bridge.is_some() {
            wps_bridge = None;
        }
        if let Some(w) = wps_bridge.as_ref() {
            app.wps_connected = w.connected();
            w.set_whiteboard(app.whiteboard);
        } else {
            app.wps_connected = false;
        }

        // 4. WPS 事件（真实页号 / 放映开始）
        if let Some(w) = wps_bridge.as_ref() {
            for ev in w.take_events() {
                match ev {
                    WpsEvent::SlideshowBegin(pos) => {
                        if app.whiteboard {
                            continue;
                        }
                        app.clear_all_pages();
                        app.wps_real_pos = pos;
                        app.current_slide = pos;
                        rt.redraw_full(&app);
                    }
                    WpsEvent::RealPos(pos) => {
                        if app.whiteboard {
                            continue;
                        }
                        if app.wps_real_pos > 0 {
                            app.current_slide = app.wps_real_pos;
                            app.save_current_page();
                        }
                        app.wps_real_pos = pos;
                        app.current_slide = pos;
                        app.load_page(pos);
                        rt.redraw_full(&app);
                    }
                }
            }
        }

        // 5. WPS 心跳
        if last_ping.elapsed() >= Duration::from_millis(1000) {
            last_ping = Instant::now();
            if let Some(w) = wps_bridge.as_ref() {
                if w.tick() {
                    app.wps_connected = false;
                    app.wps_real_pos = -1;
                }
            }
        }

        // 6. WPS 全屏检测（500ms）
        if last_wps_check.elapsed() >= Duration::from_millis(500) {
            last_wps_check = Instant::now();
            let fs = rt.x11.is_presentation_fullscreen(win);
            if fs != app.wps_fullscreen {
                app.wps_fullscreen = fs;
                if !app.whiteboard {
                    app.clear_all_pages();
                }
                rt.redraw_full(&app);
            }
            if app.mode == 0 && !app.settings_open {
                apply_input_shape(&rt, &app);
            }
        }

        // 7. 白板上限提示到点重绘
        if app.wb_limit_msg_until != 0 && now_ms() > app.wb_limit_msg_until {
            app.wb_limit_msg_until = 0;
            rt.redraw_full(&app);
        }
        if app.log_cleared_until != 0 && now_ms() > app.log_cleared_until {
            app.log_cleared_until = 0;
            if app.settings_open {
                rt.redraw_full(&app);
            }
        }

        // 8. 收缩态 2 分钟自动展开
        if app.collapsed && now_ms() - app.collapse_ts > 120_000 {
            expand_sidebars(&rt, &mut app);
        }

        std::thread::sleep(Duration::from_millis(4));
    }
}
