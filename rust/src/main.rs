// ============================================================
// Sidera - 程序入口（纯 Rust / X11，无 Qt）
// Copyright (C) 2026 Carl_Jin   GNU GPL v3
// ============================================================
mod app;
mod backend;
mod gesture;
mod paint;
mod portal;
mod splash;
mod text;
mod ui;
mod uinput;
mod wayland;
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
use x11rb::protocol::xproto::{ConnectionExt, KeyButMask};
use x11rb::protocol::Event;

use app::{now_ms, App};
use backend::{Backend, IRect, Key};
use gesture::TouchKind;
use text::Fonts;
use ui::{Btn, MoreHit, PopupHit, R, SetHit};
use wps::{WpsBridge, WpsEvent};

pub(crate) struct Ip {
    pub(crate) dragging: bool,
    pub(crate) drag_start_y: i32,
    pub(crate) drag_mouse_y0: i32,
    pub(crate) moved: bool,
    pub(crate) collapse_press: bool,
    // 触摸：正在按下的触点 id，以及每个触点是否起始于 UI（点按钮而非画线）
    pub(crate) touch_ids: HashSet<i32>,
    pub(crate) touch_over_ui: HashMap<i32, bool>,
    // 正在拖动的设置滑条
    pub(crate) slider: Option<ui::SliderId>,
    // 每个触点的接触尺寸（直径，逻辑像素）：X11 由 raw touch 填，Wayland 由 wl_touch.shape 填
    pub(crate) touch_shape: HashMap<i32, f64>,
    // 画笔中断：当前笔画是否刚经过 UI 区域（用于从侧栏出来时断开续画）
    pub(crate) stroke_skipped: bool,
}

impl Default for Ip {
    fn default() -> Self {
        Ip {
            dragging: false,
            drag_start_y: 0,
            drag_mouse_y0: 0,
            moved: false,
            collapse_press: false,
            touch_ids: HashSet::new(),
            touch_over_ui: HashMap::new(),
            slider: None,
            touch_shape: HashMap::new(),
            stroke_skipped: false,
        }
    }
}

pub(crate) struct Rt {
    pub(crate) backend: Box<dyn Backend>,
    pub(crate) fonts: Fonts,
    pub(crate) icon: Option<Pixmap>,
    pub(crate) icon_big: Option<Pixmap>,
}

impl Rt {
    pub(crate) fn redraw(&self, app: &App, r: R) {
        let x = r.x.max(0);
        let y = r.y.max(0);
        let x2 = (r.x + r.w).min(app.screen_w);
        let y2 = (r.y + r.h).min(app.screen_h);
        if x2 <= x || y2 <= y {
            return;
        }
        if let Some(pm) = ui::render_region(app, &self.fonts, self.icon.as_ref(), x, y, x2 - x, y2 - y)
        {
            self.backend.present(&pm, x, y);
        }
    }
    pub(crate) fn redraw_full(&self, app: &App) {
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
    pub(crate) fn screen_size(&self) -> (i32, i32) {
        self.backend.screen_size()
    }
    pub(crate) fn fake_key(&self, key: Key) {
        self.backend.fake_key(key);
    }
    pub(crate) fn virtual_right_click(&self, x: i32, y: i32) {
        self.backend.virtual_right_click(x, y);
    }
    pub(crate) fn has_compositor(&self) -> bool {
        self.backend.has_compositor()
    }
    pub(crate) fn shape_available(&self) -> bool {
        self.backend.shape_available()
    }
    pub(crate) fn is_fullscreen(&self) -> bool {
        self.backend.is_presentation_fullscreen()
    }
    pub(crate) fn set_input_region(&self, rects: &[IRect], full: bool) {
        self.backend.set_input_region(rects, full);
    }
}

fn rect_of(r: R, pad: i32) -> IRect {
    IRect::new(
        (r.x - pad).max(0),
        (r.y - pad).max(0),
        (r.w + pad * 2).max(0),
        (r.h + pad * 2).max(0),
    )
}

pub(crate) fn apply_input_shape(rt: &Rt, app: &App) {
    if app.confirm_quit || app.show_diag {
        rt.set_input_region(&[], true);
        return;
    }
    if app.settings_open {
        rt.set_input_region(&[rect_of(ui::settings_rect(app), 0)], false);
        return;
    }
    if app.whiteboard {
        rt.set_input_region(&[], true);
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
        rt.set_input_region(&rects, false);
    } else {
        rt.set_input_region(&[], true);
    }
}

pub(crate) fn close_all_popups(app: &mut App) {
    app.pen_popup_visible = false;
    app.eraser_popup_visible = false;
    app.more_menu_open = false;
    app.open_anim = 0;
}

/// 当前点是否落在任何 UI 元素上（侧栏/弹窗/更多菜单/设置面板）。
/// 画笔/橡皮进入这些区域应中断，避免把墨迹画到侧栏底下（半透明侧栏会透出来）。
pub(crate) fn over_ui(app: &App, x: i32, y: i32) -> bool {
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

pub(crate) fn clamp_sb(app: &mut App) {
    let h = if app.collapsed {
        app.collapsed_h()
    } else {
        app.sb_height()
    };
    app.sb_y = app.sb_y.clamp(0, (app.screen_h - h).max(0));
}

pub(crate) fn switch_to_cursor(rt: &Rt, app: &mut App) {
    close_all_popups(app);
    app.mode = 0;
    if app.is_drawing {
        paint::commit_stroke(app);
    }
    app.is_drawing = false;
    app.reset_palm_gesture();
    apply_input_shape(rt, app);
    rt.redraw_full(app);
    log::info!("[INFO] 光标模式");
}

pub(crate) fn switch_to_draw(rt: &Rt, app: &mut App, mode: i32) {
    close_all_popups(app);
    app.mode = mode;
    if app.collapsed {
        expand_sidebars(rt, app);
    }
    if app.is_drawing {
        paint::commit_stroke(app);
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

pub(crate) fn collapse_sidebars(rt: &Rt, app: &mut App) {
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

pub(crate) fn expand_sidebars(rt: &Rt, app: &mut App) {
    if !app.collapsed {
        return;
    }
    app.collapsed = false;
    clamp_sb(app);
    apply_input_shape(rt, app);
    rt.redraw_full(app);
    log::info!("[INFO] 侧边栏已展开");
}

pub(crate) fn toggle_collapse(rt: &Rt, app: &mut App) {
    if app.collapsed {
        expand_sidebars(rt, app);
    } else {
        collapse_sidebars(rt, app);
    }
}

pub(crate) fn go_prev(rt: &Rt, app: &mut App, wps: Option<&WpsBridge>) {
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
        match wps {
            Some(w) => {
                w.enqueue("PREV");
                crate::wps::wps_log("[PAGE] PREV -> 入队");
            }
            None => crate::wps::wps_log("[PAGE] PREV 但桥未运行"),
        }
        return;
    }
    crate::wps::wps_log(&format!(
        "[PAGE] PREV -> 假键 (debug={} connected={})",
        app.wps_debug, app.wps_connected
    ));
    app.save_current_page();
    if app.current_slide > 1 {
        app.current_slide -= 1;
    }
    rt.fake_key(Key::Up);
    app.load_page(app.current_slide);
    rt.redraw_full(app);
}

pub(crate) fn go_next(rt: &Rt, app: &mut App, wps: Option<&WpsBridge>) {
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
        match wps {
            Some(w) => {
                w.enqueue("NEXT");
                crate::wps::wps_log("[PAGE] NEXT -> 入队");
            }
            None => crate::wps::wps_log("[PAGE] NEXT 但桥未运行"),
        }
        return;
    }
    crate::wps::wps_log(&format!(
        "[PAGE] NEXT -> 假键 (debug={} connected={})",
        app.wps_debug, app.wps_connected
    ));
    app.save_current_page();
    app.current_slide += 1;
    rt.fake_key(Key::Down);
    app.load_page(app.current_slide);
    rt.redraw_full(app);
}

pub(crate) fn exit_presentation(rt: &Rt, app: &mut App) {
    if app.whiteboard {
        app.whiteboard_bg_index = (app.whiteboard_bg_index + 1) % app::WHITEBOARD_COLORS.len() as i32;
        rt.redraw_full(app);
        return;
    }
    if app.mode != 0 {
        switch_to_cursor(rt, app);
    }
    rt.fake_key(Key::Escape);
    log::info!("[INFO] 退出放映（已回光标模式并发送 ESC）");
}

pub(crate) fn do_screenshot(rt: &Rt) {
    match rt.backend.screenshot() {
        Some((rgba, w, h)) => {
            save_and_clipboard(rgba, w, h);
        }
        None => {
            // Wayland（无 screencopy）走 xdg-desktop-portal，异步进行
            log::info!("[INFO] 已提交门户截图任务");
            portal::screenshot_nonblocking();
        }
    }
}

/// 保存 PNG 并复制到剪贴板
pub(crate) fn save_and_clipboard(rgba: Vec<u8>, w: u32, h: u32) {
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
    // 剪贴板
    if let Ok(mut cb) = arboard::Clipboard::new() {
        let data = arboard::ImageData {
            width: w as usize,
            height: h as usize,
            bytes: std::borrow::Cow::Owned(img.into_raw()),
        };
        if cb.set_image(data).is_ok() {
            log::info!("[INFO] 截图已复制到剪贴板");
        }
    }
}

pub(crate) fn collect_system_info(rt: &Rt, app: &App) -> String {
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
        if rt.has_compositor() { "有" } else { "无" }
    ));
    s.push_str(&format!(
        "XShape: {}\n",
        if rt.shape_available() {
            "扩展可用"
        } else {
            "扩展不可用!"
        }
    ));
    s.push_str(&format!(
        "屏幕: {}x{}\n",
        rt.screen_size().0, rt.screen_size().1
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

pub(crate) fn open_settings(rt: &Rt, app: &mut App) {
    if std::env::var("SIDERA_TRACE").is_ok() { log::info!("[SETTINGS] 打开"); }
    app.settings_open = true;
    app.open_anim = now_ms();
    close_all_popups(app);
    apply_input_shape(rt, app);
    rt.redraw_full(app);
}

fn apply_slider(app: &mut App, id: ui::SliderId, v: f32) {
    match id {
        ui::SliderId::Alpha => {
            app.sidebar_alpha = v.round().clamp(30.0, 255.0) as u8;
        }
        ui::SliderId::Scale => {
            app.sb_scale = (v as f64).clamp(0.6, 1.4);
            clamp_sb(app);
        }
        ui::SliderId::Threshold => {
            app.large_touch_threshold = (v as f64).clamp(20.0, 260.0);
        }
        ui::SliderId::EraseScale => {
            app.large_erase_scale10 = (v * 10.0).round().clamp(8.0, 25.0) as i32;
        }
    }
}

pub(crate) fn handle_settings_hit(rt: &Rt, app: &mut App, hit: SetHit) {
    if std::env::var("SIDERA_TRACE").is_ok() { log::info!("[SETTINGS] 命中 {:?}", hit); }
    match hit {
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
        SetHit::ClearLog => {
            let _ = std::fs::remove_file(wps::log_file());
            app.log_cleared_until = now_ms() + 1200;
        }
        SetHit::SysInfo => {
            let info = collect_system_info(rt, app);
            log::info!("\n{}", info);
            let _ = std::fs::write("/tmp/sidera-diag.txt", &info);
            app.diag_text = info;
            app.show_diag = true;
        }
        SetHit::Quit => {
            app.confirm_quit = true;
        }
        SetHit::Done => {
            app.settings_open = false;
            app.confirm_quit = false;
            app.show_diag = false;
            apply_input_shape(rt, app);
        }
    }
    rt.redraw_full(app);
}

fn first_hit(r: R, x: i32, y: i32) -> bool {
    r.contains(x, y)
}

pub(crate) fn on_press(
    rt: &Rt,
    app: &mut App,
    ip: &mut Ip,
    wps: Option<&WpsBridge>,
    x: i32,
    y: i32,
    button: u8,
) {
    if app.confirm_quit {
        if let Some(h) = ui::confirm_hit(app, x, y) {
            match h {
                ui::ConfirmHit::Yes => std::process::exit(0),
                ui::ConfirmHit::No => {
                    app.confirm_quit = false;
                    rt.redraw_full(app);
                }
            }
        }
        return;
    }
    if app.show_diag {
        if ui::diag_close_hit(app, x, y) {
            app.show_diag = false;
        }
        rt.redraw_full(app);
        return;
    }
    log::debug!(
        "[INPUT] press ({},{}) btn={} mode={} screen={}x{}",
        x, y, button, app.mode, app.screen_w, app.screen_h
    );
    if app.settings_open {
        // 先判断滑条（拖动），再判断开关/按钮
        if let Some(def) = ui::settings_slider_hit(app, x, y) {
            apply_slider(app, def.id, def.value_at(x));
            ip.slider = Some(def.id);
            rt.redraw_full(app);
            return;
        }
        if let Some(hit) = ui::settings_hit(app, x, y) {
            handle_settings_hit(rt, app, hit);
        }
        return;
    }
    if app.more_menu_open {
        if let Some(hit) = ui::more_menu_hit(app, x, y) {
            app.more_menu_open = false;
            match hit {
                MoreHit::Screenshot => do_screenshot(rt),
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
                log::debug!("[INPUT] hit {:?} right={} rect=({},{} {}x{})", k, right, r.x, r.y, r.w, r.h);
                app.popup_on_right = right;
                handle_btn(rt, app, wps, k, right);
                return;
            }
        }
        let sb = ui::sidebar_rect(app, right);
        if sb.contains(x, y) {
            log::debug!("[INPUT] sidebar bg drag right={}", right);
            ip.dragging = true;
            ip.moved = false;
            ip.drag_start_y = app.sb_y;
            ip.drag_mouse_y0 = y;
            ip.collapse_press = app.collapsed;
            return;
        }
    }

    log::debug!("[INPUT] 未命中 UI (mode={})", app.mode);
    // 画布区域
    if app.mode != 0 {
        if button == 3 {
            if app.right_click_cursor_on && app.mode == 1 {
                rt.virtual_right_click(x, y);
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
        ip.stroke_skipped = false;
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
            app.stroke_pts.clear();
            app.stroke_pts.push((x as f32, y as f32, w as f32));
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

pub(crate) fn handle_btn(rt: &Rt, app: &mut App, wps: Option<&WpsBridge>, k: Btn, right: bool) {
    match k {
        Btn::Collapse => toggle_collapse(rt, app),
        Btn::Cursor => switch_to_cursor(rt, app),
        Btn::Pen => {
            if app.mode == 1 {
                let was = app.pen_popup_visible;
                close_all_popups(app);
                app.pen_popup_visible = !was;
                if app.pen_popup_visible {
                    app.open_anim = now_ms();
                }
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
                if app.eraser_popup_visible {
                    app.open_anim = now_ms();
                }
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
            app.open_anim = now_ms();
            apply_input_shape(rt, app);
            rt.redraw_full(app);
        }
    }
    let _ = right;
}

pub(crate) fn on_motion(rt: &Rt, app: &mut App, ip: &mut Ip, x: i32, y: i32) {
    // 悬停高亮
    let h = ui::sidebar_button_at(app, x, y);
    if h != app.hover {
        app.hover = h;
        rt.redraw_full(app);
    }
    // 设置面板滑条拖动
    if let Some(id) = ip.slider {
        if let Some(def) = ui::settings_sliders(app).into_iter().find(|s| s.id == id) {
            apply_slider(app, id, def.value_at(x));
            rt.redraw_full(app);
        }
        return;
    }
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
        let old_y = app.sb_y;
        let new_y = (ip.drag_start_y + dy).clamp(0, (app.screen_h - h).max(0));
        if new_y == old_y {
            return;
        }
        app.sb_y = new_y;
        // 只重绘左右两条侧栏窄条（新旧位置并集），不整屏也不整宽
        let sbw = app.sb_width();
        let y0 = (old_y.min(new_y) - 8).max(0);
        let y1 = ((old_y.max(new_y) + h) + 8).min(app.screen_h);
        let hh = (y1 - y0).max(1);
        for right in [false, true] {
            let sbx = if right {
                app.sb_x_right()
            } else {
                app.sb_x_left()
            };
            rt.redraw(
                app,
                R {
                    x: sbx - 8,
                    y: y0,
                    w: sbw + 16,
                    h: hh,
                },
            );
        }
        // 拖动过程中不更新输入区域（指针已被隐式抓取，松手后再更新）
        return;
    }
    if !app.is_drawing || app.mode == 0 {
        return;
    }
    let cur = (x, y);
    let erase = app.mode == 2;
    let w = if erase {
        app.eraser_width()
    } else {
        app.timed_pen_width()
    };
    // 笔尖在 UI 上：暂停本笔画（不画、不动 last_pt），出了 UI 再续
    if over_ui(app, x, y) {
        ip.stroke_skipped = true;
        return;
    }
    if ip.stroke_skipped {
        // 刚从 UI 出来：结束上一段，从当前点新起一段（避免跨侧栏拉一条线）
        if !erase {
            paint::commit_stroke(app);
        }
        ip.stroke_skipped = false;
        app.last_pen_w = w;
        app.last_pt = cur;
        if erase {
            paint::stroke_segment(app, cur, cur, true, w);
        } else {
            app.stroke_pts.clear();
            app.stroke_pts.push((cur.0 as f32, cur.1 as f32, w as f32));
        }
        rt.redraw(
            app,
            R {
                x: cur.0 - w,
                y: cur.1 - w,
                w: w * 2,
                h: w * 2,
            },
        );
        return;
    }
    let prev = app.last_pt;
    if erase {
        paint::stroke_segment(app, prev, cur, true, w);
    } else {
        app.stroke_pts.push((cur.0 as f32, cur.1 as f32, w as f32));
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

pub(crate) fn on_release(rt: &Rt, app: &mut App, ip: &mut Ip, _x: i32, _y: i32) {
    if ip.slider.is_some() {
        ip.slider = None;
        app::save_settings(app);
    }
    if app.is_drawing {
        app.is_drawing = false;
        paint::commit_stroke(app);
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
        } else if ip.moved {
            // 拖动结束：按新位置更新输入区域
            apply_input_shape(rt, app);
        }
    }
}

/// 触摸事件（X11 与 Wayland 共用）
pub(crate) fn handle_touch(
    rt: &Rt,
    app: &mut App,
    ip: &mut Ip,
    wps: Option<&WpsBridge>,
    id: i32,
    x: i32,
    y: i32,
    kind: TouchKind,
    diameter: f64,
) {
    match kind {
        TouchKind::Begin => {
            ip.touch_ids.insert(id);
            if over_ui(app, x, y) {
                ip.touch_over_ui.insert(id, true);
                on_press(rt, app, ip, wps, x, y, 1);
                ip.dragging = false;
            } else {
                ip.touch_over_ui.insert(id, false);
                if let Some(_r) = gesture::touch_event(app, id, x, y, TouchKind::Begin, diameter) {
                    rt.redraw_full(app);
                }
            }
        }
        TouchKind::Update => {
            if ip.slider.is_some() {
                on_motion(rt, app, ip, x, y);
                return;
            }
            if ip.touch_over_ui.get(&id).copied().unwrap_or(false) {
                return;
            }
            if over_ui(app, x, y) {
                app.reset_palm_gesture();
                return;
            }
            if let Some(r) = gesture::touch_event(app, id, x, y, TouchKind::Update, diameter) {
                rt.redraw(app, r);
            }
        }
        TouchKind::End => {
            ip.touch_ids.remove(&id);
            let over = ip.touch_over_ui.remove(&id).unwrap_or(false);
            if ip.slider.is_some() {
                ip.slider = None;
                app::save_settings(app);
            }
            if over {
                return;
            }
            if let Some(r) = gesture::touch_event(app, id, x, y, TouchKind::End, 0.0) {
                rt.redraw(app, r);
            }
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
        Event::LeaveNotify(_) => {
            if app.hover.is_some() {
                app.hover = None;
                rt.redraw_full(app);
            }
        }
        Event::KeyPress(e) => {
            let mods = KeyButMask::CONTROL | KeyButMask::SHIFT;
            let key_d = rt
                .backend
                .as_any()
                .downcast_ref::<x11::X11Plat>()
                .map(|p| p.x11.key_d)
                .unwrap_or(0);
            if e.detail == key_d && (e.state & mods) == mods {
                if app.mode != 0 {
                    switch_to_cursor(rt, app);
                } else {
                    switch_to_draw(rt, app, 1);
                }
            }
        }
        Event::XinputRawTouchBegin(e) | Event::XinputRawTouchUpdate(e) => {
            if let Some(xp) = rt.backend.as_any().downcast_ref::<x11::X11Plat>() {
                let d = xp.touch_diameter(e.deviceid as u8, &e.valuator_mask, &e.axisvalues_raw);
                if d > 0.0 {
                    ip.touch_shape.insert(e.detail as i32, d);
                }
            }
        }
        Event::XinputRawTouchEnd(e) => {
            ip.touch_shape.remove(&(e.detail as i32));
        }
        Event::XinputTouchBegin(e) => {
            let x = (e.event_x as f64 / 65536.0).round() as i32;
            let y = (e.event_y as f64 / 65536.0).round() as i32;
            let d = ip.touch_shape.get(&(e.detail as i32)).copied().unwrap_or_else(|| {
                rt.backend
                    .as_any()
                    .downcast_ref::<x11::X11Plat>()
                    .map(|xp| xp.touch_diameter(e.deviceid as u8, &e.valuator_mask, &e.axisvalues))
                    .unwrap_or(0.0)
            });
            handle_touch(rt, app, ip, wps, e.detail as i32, x, y, TouchKind::Begin, d);
        }
        Event::XinputTouchUpdate(e) => {
            let x = (e.event_x as f64 / 65536.0).round() as i32;
            let y = (e.event_y as f64 / 65536.0).round() as i32;
            let d = ip.touch_shape.get(&(e.detail as i32)).copied().unwrap_or_else(|| {
                rt.backend
                    .as_any()
                    .downcast_ref::<x11::X11Plat>()
                    .map(|xp| xp.touch_diameter(e.deviceid as u8, &e.valuator_mask, &e.axisvalues))
                    .unwrap_or(0.0)
            });
            handle_touch(rt, app, ip, wps, e.detail as i32, x, y, TouchKind::Update, d);
        }
        Event::XinputTouchEnd(e) => {
            let x = (e.event_x as f64 / 65536.0).round() as i32;
            let y = (e.event_y as f64 / 65536.0).round() as i32;
            handle_touch(rt, app, ip, wps, e.detail as i32, x, y, TouchKind::End, 0.0);
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
            for up in 0..5 {
                let mut d = dir.to_path_buf();
                for _ in 0..up {
                    d = d.join("..");
                }
                cands.push(d.join("sidera.png"));
            }
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        for up in 0..3 {
            let mut d = cwd.clone();
            for _ in 0..up {
                d = d.join("..");
            }
            cands.push(d.join("sidera.png"));
        }
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

pub(crate) struct Timers {
    last_wps: Instant,
    last_ping: Instant,
}

impl Timers {
    pub(crate) fn new() -> Self {
        Timers {
            last_wps: Instant::now() - Duration::from_secs(10),
            last_ping: Instant::now(),
        }
    }
}

/// 与具体后端无关的周期性任务：WPS 桥/事件/心跳/全屏检测、白板提示、侧栏自动展开
pub(crate) fn tick(rt: &Rt, app: &mut App, wps_bridge: &mut Option<WpsBridge>, t: &mut Timers) {
    // WPS 桥存在性与设置同步
    if app.wps_debug && wps_bridge.is_none() {
        *wps_bridge = WpsBridge::start();
    } else if !app.wps_debug && wps_bridge.is_some() {
        *wps_bridge = None;
    }
    if let Some(w) = wps_bridge.as_ref() {
        app.wps_connected = w.connected();
        w.set_whiteboard(app.whiteboard);
    } else {
        app.wps_connected = false;
    }

    // WPS 事件（真实页号 / 放映开始）
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
                    rt.redraw_full(app);
                }
                WpsEvent::SlideshowEnd => {
                    app.clear_all_pages();
                    app.wps_real_pos = -1;
                    rt.redraw_full(app);
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
                    rt.redraw_full(app);
                }
            }
        }
    }



    // 心跳
    if t.last_ping.elapsed() >= Duration::from_millis(1000) {
        t.last_ping = Instant::now();
        if let Some(w) = wps_bridge.as_ref() {
            if w.tick() {
                app.wps_connected = false;
                app.wps_real_pos = -1;
            }
        }
    }

    // 全屏检测（500ms）
    if t.last_wps.elapsed() >= Duration::from_millis(500) {
        t.last_wps = Instant::now();
        let fs = rt.is_fullscreen();
        if fs != app.wps_fullscreen {
            app.wps_fullscreen = fs;
            if !app.whiteboard {
                app.clear_all_pages();
            }
            rt.redraw_full(app);
        }
        if app.mode == 0 && !app.settings_open {
            apply_input_shape(rt, app);
        }
    }

    // 白板上限提示到点重绘
    if app.wb_limit_msg_until != 0 && now_ms() > app.wb_limit_msg_until {
        app.wb_limit_msg_until = 0;
        rt.redraw_full(app);
    }
    if app.log_cleared_until != 0 && now_ms() > app.log_cleared_until {
        app.log_cleared_until = 0;
        if app.settings_open {
            rt.redraw_full(app);
        }
    }

    // 收缩态 2 分钟自动展开
    if app.collapsed && now_ms() - app.collapse_ts > 120_000 {
        expand_sidebars(rt, app);
    }

    // 弹窗/菜单淡入动画期间持续重绘
    if app.open_anim != 0 && now_ms() < app.open_anim + 180 {
        rt.redraw_full(app);
    }
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // 软件渲染（无 GPU 老机器）
    std::env::set_var("LIBGL_ALWAYS_SOFTWARE", "1");

    let wayland = std::env::var("XDG_SESSION_TYPE")
        .map(|v| v.to_lowercase() == "wayland")
        .unwrap_or(false)
        || std::env::var("WAYLAND_DISPLAY").is_ok();

    let single = single_instance();
    if single.is_none() {
        return;
    }

    let fonts = Fonts::load();
    let icon_big = load_icon(84);
    let icon = load_icon(44);
    if icon.is_some() {
        log::info!("[INFO] 已加载软件图标");
    } else {
        log::warn!("[WARN] 未找到 sidera.png，设置/闪屏将无图标（可设 SIDERA_ICON 指定）");
    }

    if wayland {
        log::info!("[INFO] 平台: wayland");
        wayland::run(fonts, icon, icon_big, single);
        return;
    }

    let x11 = match x11::X11::connect() {
        Ok(x) => x,
        Err(e) => {
            log::error!("[ERROR] 无法连接 X11: {}", e);
            return;
        }
    };
    log::info!("[INFO] 平台: x11  屏幕 {}x{}", x11.width, x11.height);

    let (win, gc) = match x11.create_overlay() {
        Ok(v) => v,
        Err(e) => {
            log::error!("[ERROR] 创建覆盖层失败: {}", e);
            return;
        }
    };

    // XInput2 触摸事件（若可用）
    {
        let mask = XIEventMask::TOUCH_BEGIN
            | XIEventMask::TOUCH_UPDATE
            | XIEventMask::TOUCH_END
            | XIEventMask::RAW_TOUCH_BEGIN
            | XIEventMask::RAW_TOUCH_UPDATE
            | XIEventMask::RAW_TOUCH_END;
        let em = xinput::EventMask {
            deviceid: 0,
            mask: vec![mask],
        };
        if let Err(e) = xinput::xi_select_events(&x11.conn, win, &[em]) {
            log::warn!("[WARN] XInput2 触摸选择失败（触摸可能不可用）: {}", e);
        }
    }


    let mut canvas = Pixmap::new(x11.width as u32, x11.height as u32).unwrap();
    canvas.fill(tiny_skia::Color::TRANSPARENT);
    let mut app = App::new(canvas, x11.width, x11.height);
    app::load_settings(&mut app);
    // 环境变量仅本次运行覆盖（不落盘），与 C++ 版一致
    if let Ok(v) = std::env::var("WPS_API_DEBUG") {
        app.wps_debug = v == "1";
    }
    clamp_sb(&mut app);

    let rt = Rt {
        backend: Box::new(x11::X11Plat::new(x11, win, gc)),
        fonts,
        icon,
        icon_big,
    };

    let xp = rt
        .backend
        .as_any()
        .downcast_ref::<x11::X11Plat>()
        .expect("x11 backend");
    app.hotkey_ok = rt.backend.grab_hotkey();
    let x11 = &xp.x11;
    let win = xp.win;
    let _gc = xp.gc;

    if !x11.has_compositor() {
        log::warn!(
            "[WARN] 未检测到 X11 合成器！透明画布将无法显示。请开启合成器，例如: picom & 或 xcompmgr &"
        );
    }

    // 冒烟测试：渲染一帧后立即退出（用于无显示环境自检）
    if std::env::var("SIDERA_SMOKE").is_ok() {
        log::info!("[SMOKE] 渲染一帧后退出");
        rt.redraw_full(&app);
        std::thread::sleep(Duration::from_millis(200));
        x11.destroy_window(win);
        return;
    }

    // 调试：画一条穿过侧边栏的笔迹并保存整屏 PNG，用于检查渲染顺序
    if let Ok(path) = std::env::var("SIDERA_DEBUG_DRAW") {
        app.mode = 1;
        paint::stroke_tapered(&mut app, (0, 800), (400, 800), 6, 6);
        paint::stroke_tapered(&mut app, (30, 620), (30, 900), 6, 6);
        paint::stroke_tapered(&mut app, (2500, 800), (2560, 800), 6, 6);
        // 模拟一条密集的鼠标曲线（多采样点并集）
        let mut pts: Vec<(f32, f32, f32)> = Vec::new();
        for i in 0..220 {
            let t = i as f32 / 219.0;
            let x = 300.0 + t * 700.0;
            let y = 900.0 + (t * 12.0).sin() * 90.0;
            pts.push((x, y, 10.0));
        }
        let c = app.pen_color();
        paint::draw_stroke_union(&mut app.canvas, c, &pts, tiny_skia::Transform::identity());
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
        x11.destroy_window(win);
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
        x11.destroy_window(win);
        return;
    }

    // 调试：查询窗口的 XShape 输入区域
    if std::env::var("SIDERA_DEBUG_SHAPE").is_ok() {
        use x11rb::protocol::shape;
        apply_input_shape(&rt, &app);
        std::thread::sleep(Duration::from_millis(200));
        match shape::get_rectangles(&x11.conn, win, shape::SK::INPUT)
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
        if let Some(r) = shape::get_rectangles(&x11.conn, win, shape::SK::INPUT)
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
        x11.destroy_window(win);
        return;
    }

    // 调试：验证 XTest 指针移动是否生效（判断 XWayland 下能否注入事件）
    if std::env::var("SIDERA_TEST_XTEST").is_ok() {
        use x11rb::protocol::xproto;
        let before = xproto::query_pointer(&x11.conn, x11.root)
            .ok()
            .and_then(|c| c.reply().ok());
        x11.virtual_right_click(123, 456); // 内部会先 fake motion
        std::thread::sleep(Duration::from_millis(300));
        let after = xproto::query_pointer(&x11.conn, x11.root)
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
        x11.destroy_window(win);
        return;
    }

    // 启动闪屏
    if std::env::var("SIDERA_NO_SPLASH").is_err() {
        rt.backend.show_splash(&rt.fonts, rt.icon_big.as_ref(), 2000);
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

    let mut ip = Ip::default();


    let mut timers = Timers::new();

    loop {
        // 1. X 事件
        loop {
            match x11.conn.poll_for_event() {
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
                let _ = x11.conn.configure_window(
                    win,
                    &x11rb::protocol::xproto::ConfigureWindowAux::new()
                        .stack_mode(x11rb::protocol::xproto::StackMode::ABOVE),
                );
                let _ = x11.conn.map_window(win);
                let _ = x11.conn.flush();
                rt.redraw_full(&app);
            }
        }

        tick(&rt, &mut app, &mut wps_bridge, &mut timers);

        std::thread::sleep(Duration::from_millis(4));
    }
}
