// ============================================================
// Sidera - 程序入口（纯 Rust / X11，无 Qt）
// Copyright (C) 2026 Carl_Jin   GNU GPL v3
// ============================================================
mod app;
mod backend;
mod bridge_http;
mod bridge_ws;
mod clear_history;
mod desktop;
mod diag_log;
mod clipboard;
mod gnome;
mod gesture;
mod instance;
mod paint;
mod persistence;
mod portal;
mod launch;
mod splash_paint;
mod startup;
mod text;
mod touch_evdev;
mod toolbar;
mod page_picker;
mod page_cache;
mod ui;
mod ui_icons;
mod ui_theme;
mod system_theme;
mod uinput;
mod wayland;
mod wps;
mod x11;
#[cfg(test)]
mod navigation_tests;

use std::collections::{HashMap, HashSet};
use std::io::Read;
use instance::Instance as UnixListener;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use tiny_skia::{IntSize, Pixmap};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt, KeyButMask};
use x11rb::protocol::Event;

use app::{now_ms, App};
use backend::{Backend, IRect, Key};
use gesture::TouchKind;
use text::Fonts;
use ui::{Btn, MoreHit, PopupHit, R, SetHit};
use wps::{WpsBridge, WpsEvent};

/// input 组授权弹窗状态：0=待授权 1=授权中 2=成功 3=失败（后台线程更新）
static INPUT_GROUP_STATE: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

fn current_user() -> String {
    if let Ok(u) = std::env::var("USER") {
        if !u.is_empty() {
            return u;
        }
    }
    let uid = unsafe { libc::getuid() };
    if let Ok(p) = std::fs::read_to_string("/etc/passwd") {
        for line in p.lines() {
            let f: Vec<&str> = line.split(':').collect();
            if f.len() >= 3 && f[2].parse::<u32>() == Ok(uid) {
                return f[0].to_string();
            }
        }
    }
    String::from("$USER")
}

/// 后台用 pkexec 把当前用户加入 input 组（弹系统授权框输入密码）
fn start_input_group_auth(app: &mut App) {
    use std::sync::atomic::Ordering;
    app.input_group_state = 1;
    INPUT_GROUP_STATE.store(1, Ordering::Relaxed);
    let user = current_user();
    std::thread::spawn(move || {
        let res = std::process::Command::new("pkexec")
            .args(["usermod", "-aG", "input", &user])
            .status();
        let state = match res {
            Ok(s) if s.success() => {
                log::info!("[INFO] 已将 {} 加入 input 组，注销重登后生效", user);
                2
            }
            Ok(s) => {
                log::warn!("[WARN] pkexec usermod 失败，退出码 {:?}", s.code());
                3
            }
            Err(e) => {
                log::warn!(
                    "[WARN] 无法运行 pkexec: {}（可手动执行: sudo usermod -aG input {}）",
                    e,
                    user
                );
                3
            }
        };
        INPUT_GROUP_STATE.store(state, Ordering::Relaxed);
    });
}

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
    // 最近一次触摸事件时间（用于兜底清理漏掉的 TouchEnd）
    pub(crate) last_touch_ms: i64,
    // evdev 触摸是否启用；是否完全忽略 X 合成鼠标；最近触摸位置；被忽略(穿透)的触点
    pub(crate) evdev_touch: bool,
    pub(crate) ignore_core_pointer: bool,
    pub(crate) last_touch_pos: (i32, i32),
    pub(crate) ignored_touch: HashSet<i32>,
    pub(crate) pending_right_click: Option<(i32, i32)>,
    pub(crate) pending_ui: Option<ui::Control>,
    pub(crate) ui_touch: Option<i32>,
    pub(crate) rejected_touch: HashSet<i32>,
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
            last_touch_ms: 0,
            evdev_touch: false,
            ignore_core_pointer: false,
            last_touch_pos: (0, 0),
            ignored_touch: HashSet::new(),
            pending_right_click: None,
            pending_ui: None,
            ui_touch: None,
            rejected_touch: HashSet::new(),
        }
    }
}

pub(crate) struct Rt {
    pub(crate) backend: Box<dyn Backend>,
    pub(crate) fonts: Fonts,
    pub(crate) icon: Option<Pixmap>,
    pub(crate) icon_big: Option<Pixmap>,
    // 重绘合并：事件处理期间只累积脏区，一帧结束时统一渲染一次，避免事件积压
    batching: std::cell::Cell<bool>,
    dirty: std::cell::RefCell<Vec<R>>,
}

impl Rt {
    /// 真正渲染并上屏一个区域
    fn render(&self, app: &App, r: R) {
        let x = r.x.max(0);
        let y = r.y.max(0);
        let x2 = (r.x + r.w).min(app.screen_w);
        let y2 = (r.y + r.h).min(app.screen_h);
        if x2 <= x || y2 <= y {
            return;
        }
        let icon = if app.launch.active() { self.icon_big.as_ref() } else { self.icon.as_ref() };
        if let Some(pm) = ui::render_region(app, &self.fonts, icon, x, y, x2 - x, y2 - y)
        {
            self.backend.present(&pm, x, y);
        }
    }
    /// 请求重绘一个区域：批处理模式下只登记脏区，否则立即渲染
    pub(crate) fn redraw(&self, app: &App, r: R) {
        if self.batching.get() {
            let x = r.x.max(0);
            let y = r.y.max(0);
            let x2 = (r.x + r.w).min(app.screen_w);
            let y2 = (r.y + r.h).min(app.screen_h);
            if x2 > x && y2 > y {
                self.dirty.borrow_mut().push(R {
                    x,
                    y,
                    w: x2 - x,
                    h: y2 - y,
                });
            }
        } else {
            self.render(app, r);
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
    /// 开启批处理（清空上一帧残留）
    pub(crate) fn begin_batch(&self) {
        self.dirty.borrow_mut().clear();
        self.batching.set(true);
    }
    /// 合并相交脏区；左右工具栏分别刷新，避免动画重绘整屏宽度。
    pub(crate) fn flush_dirty(&self, app: &App) {
        let rects: Vec<R> = std::mem::take(&mut *self.dirty.borrow_mut());
        for r in merge_redraw_regions(rects) { self.render(app, r); }
    }
    /// 结束批处理并渲染
    pub(crate) fn end_batch(&self, app: &App) {
        self.batching.set(false);
        self.flush_dirty(app);
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
    pub(crate) fn wps_foreground(&self) -> Option<bool> {
        self.backend.presentation_foreground()
    }
    pub(crate) fn set_input_region(&self, rects: &[IRect], full: bool) {
        self.backend.set_input_region(rects, full);
    }
}

fn merge_redraw_regions(rects: Vec<R>) -> Vec<R> {
    let mut regions: Vec<R> = Vec::new();
    for mut r in rects {
        let mut i = 0;
        while i < regions.len() {
            let other = regions[i];
            if r.x <= other.x + other.w && other.x <= r.x + r.w
                && r.y <= other.y + other.h && other.y <= r.y + r.h {
                r = gesture::union(Some(r), regions.swap_remove(i));
                i = 0;
            } else { i += 1; }
        }
        regions.push(r);
        // Bound fragmented drawing damage without penalizing the two sidebars.
        if regions.len() > 32 {
            let all = regions.drain(..).reduce(|a, b| gesture::union(Some(a), b)).unwrap();
            regions.push(all);
        }
    }
    regions
}

fn rect_of(app: &App, r: R, pad: i32) -> Option<IRect> {
    // Clip both ends instead of moving an offscreen rectangle onto the desktop.
    if r.w <= 0 || r.h <= 0 { return None; }
    let x = (r.x - pad).clamp(0, app.screen_w);
    let y = (r.y - pad).clamp(0, app.screen_h);
    let right = (r.x + r.w + pad).clamp(0, app.screen_w);
    let bottom = (r.y + r.h + pad).clamp(0, app.screen_h);
    (right > x && bottom > y).then(|| IRect::new(x, y, right - x, bottom - y))
}

pub(crate) fn apply_input_shape(rt: &Rt, app: &App) {
    if app.launch.active() {
        let mut rects = Vec::new();
        if app.launch.panel_alpha() > 0.0 { rects.extend(rect_of(app, ui::launch_rect(app), 0)); }
        if app.launch.reveal() > 0.0 {
            // Reserve the two short-lived reveal strips, without remapping an
            // XWayland input window on every animation frame. Actions wait.
            for right in [false, true] { rects.extend(rect_of(app, ui::sidebar_rect(app, right), 4)); }
        }
        rt.set_input_region(&rects, false);
        return;
    }
    if app.confirm_quit || app.show_diag || app.show_input_group_prompt {
        rt.set_input_region(&[], true);
        return;
    }
    if app.settings_open {
        let rects: Vec<_> = rect_of(app, ui::settings_rect(app), 0).into_iter().collect();
        rt.set_input_region(&rects, false);
        return;
    }
    if app.whiteboard {
        rt.set_input_region(&[], true);
        return;
    }
    if app.mode == 0 {
        let mut rects = Vec::new();
        for right in [false, true] {
            rects.extend(rect_of(app, ui::sidebar_input_rect(app, right), 4));
        }
        if app.pen_popup_visible {
            rects.extend(rect_of(app, ui::pen_popup_rect(app), 4));
        }
        if app.eraser_popup_visible {
            rects.extend(rect_of(app, ui::eraser_popup_rect(app), 4));
        }
        if app.more_menu_open {
            rects.extend(rect_of(app, ui::more_menu_rect(app), 4));
        }
        if app.page_picker.is_some() { rects.extend(rect_of(app, ui::page_picker_rect(app), 4)); }
        rt.set_input_region(&rects, false);
    } else {
        rt.set_input_region(&[], true);
    }
}

pub(crate) fn close_all_popups(rt: &Rt, app: &mut App) {
    // Capture the old bounds before hiding the panels. A stroke only redraws
    // its own small area; forgetting these bounds leaves a torn popup behind.
    let regions = [
        app.pen_popup_visible.then(|| ui::pen_popup_rect(app)),
        app.eraser_popup_visible.then(|| ui::eraser_popup_rect(app)),
        app.more_menu_open.then(|| ui::more_menu_rect(app)),
        app.page_picker.as_ref().map(|_| ui::page_picker_rect(app)),
    ];
    app.pen_popup_visible = false;
    app.eraser_popup_visible = false;
    app.more_menu_open = false;
    if app.page_picker.take().is_some() {
        for right in [false, true] { rt.redraw(app, ui::sidebar_page_rect(app, right)); }
    }
    app.open_anim = 0;
    if regions.iter().any(Option::is_some) {
        apply_input_shape(rt, app);
        for r in regions.into_iter().flatten() { rt.redraw(app, r); }
    }
}

/// 当前点是否落在任何 UI 元素上（侧栏/弹窗/更多菜单/设置面板）。
/// 画笔/橡皮进入这些区域应中断，避免把墨迹画到侧栏底下（半透明侧栏会透出来）。
pub(crate) fn over_ui(app: &App, x: i32, y: i32) -> bool {
    if app.confirm_quit || app.show_diag { return true; }
    if app.page_picker.is_some() && ui::page_picker_rect(app).contains(x, y) { return true; }
    for right in [false, true] {
        if ui::sidebar_input_rect(app, right).contains(x, y) {
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

/// 该逻辑坐标是否属于覆盖层“可交互”区域（与 apply_input_shape 一致）。
/// evdev 是全局读取，需要用它模拟输入区域，实现光标模式下的点击穿透。
pub(crate) fn point_input_ok(app: &App, x: i32, y: i32) -> bool {
    if app.confirm_quit || app.show_diag || app.show_input_group_prompt || app.whiteboard {
        return true;
    }
    if app.settings_open {
        return ui::settings_rect(app).contains(x, y);
    }
    if app.mode != 0 {
        return true;
    }
    over_ui(app, x, y)
}

pub(crate) fn switch_to_cursor(rt: &Rt, app: &mut App) {
    if app.launch.active() { return; }
    if app.whiteboard { return; }
    close_all_popups(rt, app);
    app.mode = 0;
    if app.is_drawing {
        paint::commit_stroke(app);
    }
    app.is_drawing = false;
    app.reset_palm_gesture();
    app.request_drawing_tools_collapse();
    apply_input_shape(rt, app);
    rt.redraw_full(app);
    log::info!("[INFO] 光标模式");
}

pub(crate) fn switch_to_draw(rt: &Rt, app: &mut App, mode: i32) {
    if app.launch.active() { return; }
    close_all_popups(rt, app);
    app.mode = mode;
    app.drawing_tools.set_expanded(true);
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
    close_all_popups(rt, app);
    app.drawing_tools.set_expanded(false);
    app.set_sidebar_collapsed(true);
    if app.mode != 0 {
        switch_to_cursor(rt, app);
    }
    apply_input_shape(rt, app);
    rt.redraw_full(app);
    log::info!("[INFO] 侧边栏已收缩");
}

pub(crate) fn expand_sidebars(rt: &Rt, app: &mut App) {
    if !app.collapsed {
        return;
    }
    app.set_sidebar_collapsed(false);
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

#[derive(Debug, PartialEq)]
enum NavigationRoute { Wps, DirectKeys, Unavailable }

fn navigation_route(linked_show: bool, bridge_ready: bool, direct_keys: bool) -> NavigationRoute {
    if linked_show {
        if bridge_ready { NavigationRoute::Wps } else { NavigationRoute::Unavailable }
    } else if direct_keys {
        NavigationRoute::DirectKeys
    } else {
        NavigationRoute::Unavailable
    }
}

// 未接入放映时恢复模拟键盘；已接入的放映只走 API，失败也不重复发键。
// 后端必须提供免弹窗输入：XWayland 使用 uinput，不能退回 XTEST/Portal。
fn external_navigation(rt: &Rt, app: &mut App, wps: Option<&WpsBridge>, cmd: &str, key: Key) -> bool {
    let connected = app.wps_debug && wps.is_some_and(|w| w.connected());
    if app.wps_connected != connected {
        close_all_popups(rt, app);
        app.wps_connected = connected;
        apply_input_shape(rt, app);
        rt.redraw_full(app);
    }
    match navigation_route(app.wps_debug && wps.is_some_and(|w| w.showing()),
                           wps.is_some_and(|w| w.ready()),
                           rt.backend.can_inject_keys_without_prompt()) {
        NavigationRoute::Wps => {
            app.wps_navigation_ready = wps.is_some_and(|w| w.enqueue(cmd));
        }
        NavigationRoute::DirectKeys => {
            app.wps_navigation_ready = true;
            rt.fake_key(key);
            return true;
        }
        NavigationRoute::Unavailable => {
            app.wps_navigation_ready = false;
            crate::wps::wps_log(&format!("[PAGE] {} 未执行：联动未就绪或当前后端需要输入授权", cmd));
        }
    }
    rt.redraw_full(app);
    false
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
    if !external_navigation(rt, app, wps, "PREV", Key::Up) {
        return;
    }
    app.save_current_page();
    if app.current_slide > 1 {
        app.current_slide -= 1;
    }
    app.load_page(app.current_slide);
    rt.redraw_full(app);
}

fn open_page_picker(rt: &Rt, app: &mut App, right: bool) {
    let Some((page, total, source)) = app.page_picker_context() else { return };
    close_all_popups(rt, app);
    app.popup_on_right = right;
    app.page_picker = Some(page_picker::PagePicker::new(page, total, source));
    app.open_anim = now_ms();
    apply_input_shape(rt, app);
    rt.redraw(app, ui::page_picker_rect(app));
    rt.redraw(app, ui::sidebar_page_rect(app, right));
}

fn commit_page_picker(rt: &Rt, app: &mut App, wps: Option<&WpsBridge>) {
    let Some(picker) = &app.page_picker else { return };
    let (page, total, source) = (picker.selected(), picker.total, picker.source);
    let context = app.page_picker_context();
    close_all_popups(rt, app);
    let Some((current, live_total, live_source)) = context else { return };
    if total != live_total || source != live_source || page == current { return; }
    match source {
        page_picker::Source::Whiteboard => {
            app.save_current_page();
            app.current_slide = page;
            app.load_page(page);
            rt.redraw_full(app);
        }
        page_picker::Source::Wps(epoch) => {
            // A single absolute command; real WPS state moves the ink cache.
            // Never approximate a jump with repeated next/previous keystrokes.
            if let Some(bridge) = wps { bridge.enqueue_jump(page, epoch); }
        }
    }
}

pub(crate) fn on_scroll(rt: &Rt, app: &mut App, ip: &Ip, x: i32, y: i32, pages: f64) {
    if app.confirm_quit || app.show_diag || ip.pending_ui.is_some()
        || ip.slider.is_some() || ip.dragging { return; }
    if app.settings_open {
        if pages.is_finite() && pages != 0.0 && ui::settings_rect(app).contains(x, y)
            && ui::settings_change_page(app, if pages > 0.0 { 1 } else { -1 }) {
            rt.redraw(app, ui::settings_rect(app));
        }
        return;
    }
    if app.page_picker.is_none() || !ui::page_picker_control_rect(app, ui::PagePickerHit::Wheel).contains(x, y) { return; }
    if let Some(picker) = app.page_picker.as_mut() { picker.scroll(pages, Instant::now()); }
    rt.redraw(app, ui::page_picker_rect(app));
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
    if !external_navigation(rt, app, wps, "NEXT", Key::Down) {
        return;
    }
    app.save_current_page();
    app.current_slide += 1;
    app.load_page(app.current_slide);
    rt.redraw_full(app);
}

pub(crate) fn exit_presentation(rt: &Rt, app: &mut App, wps: Option<&WpsBridge>) {
    if app.whiteboard {
        app.whiteboard_bg_index = (app.whiteboard_bg_index + 1) % app::WHITEBOARD_COLORS.len() as i32;
        rt.redraw_full(app);
        return;
    }
    if app.mode != 0 {
        switch_to_cursor(rt, app);
    }
    if app.wps_debug && !app.wps_test_mode {
        // Default mode deliberately avoids the WPS command queue for entering
        // or leaving a show. Keyboard shortcuts still work when the add-in is
        // slow or its API is blocked.
        if wps.is_some_and(|w| w.showing()) {
            rt.fake_key(Key::Escape);
        } else if now_ms() >= app.wps_start_sent_until {
            if !rt.backend.start_presentation() {
                crate::diag_log::warn("WPS 默认模式无法注入 Shift+F5，开始放映失败");
            } else {
                app.wps_start_sent_until = now_ms() + 2500;
                crate::diag_log::info("WPS 默认模式已发送 Shift+F5 开始放映");
            }
        }
        rt.redraw_full(app);
        return;
    }
    if app.wps_debug && wps.is_some_and(|w| w.start_available()) {
        // START has its own guard against duplicate taps; never send Esc here.
        if let Some(w) = wps { w.enqueue("START"); }
        rt.redraw_full(app);
        return;
    }
    external_navigation(rt, app, wps, "EXIT", Key::Escape);
}

pub(crate) fn do_screenshot(rt: &Rt) {
    if rt.backend.request_screenshot() { return; }
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
    clipboard::copy_image(img.into_raw(), w, h);
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
    s.push_str(&format!("显示后端: {}\n", rt.backend.description()));
    s.push_str(&format!("界面主题: {}（当前{}）\n", app.theme.preference.label(),
        if app.theme.dark() { "深色" } else { "浅色" }));
    s.push_str(&format!("直接翻页按键: {}\n右键转发: {}\nCtrl+Shift+D: {}\n",
        rt.backend.can_inject_keys_without_prompt(), rt.backend.can_forward_right_click(), app.hotkey_ok));
    s.push_str(&format!(
        "点击区域控制: {}\n",
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
    s.push_str(&format!("WPS 模式: {}\n", if app.wps_test_mode { "测试（页码/跳页/开始）" } else { "默认（API 翻页 + 快捷键）" }));
    if let Some(path) = diag_log::path() {
        s.push_str(&format!("诊断日志: {}\n", path.display()));
    }
    s.push_str("\n最近日志：\n");
    for line in diag_log::tail(10) { s.push_str(&line); s.push('\n'); }
    s
}

pub(crate) fn open_settings(rt: &Rt, app: &mut App) {
    if std::env::var("SIDERA_TRACE").is_ok() { log::info!("[SETTINGS] 打开"); }
    app.settings_open = true;
    app.settings_page = 0;
    app.open_anim = now_ms();
    close_all_popups(rt, app);
    apply_input_shape(rt, app);
    rt.redraw_full(app);
}

fn apply_slider(app: &mut App, id: ui::SliderId, v: f32) {
    match id {
        ui::SliderId::Alpha => {
            app.sidebar_alpha = v.round().clamp(30.0, 255.0) as u8;
        }
        ui::SliderId::Scale => {
            // Keep exact percent steps stable across pixel rounding and saved settings.
            app.sb_scale = (f64::from(v) * 100.0).round().clamp(60.0, 140.0) / 100.0;
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
        SetHit::Theme(preference) => {
            app.apply_theme(ui_theme::Theme { preference, system_dark: system_theme::dark(), ..app.theme });
            app::save_settings(app);
        }
        SetHit::ResetSlider(id) => {
            apply_slider(app, id, id.default_value());
            app::save_settings(app);
        }
        SetHit::Autostart => ui::set_autostart(!ui::is_autostart()),
        SetHit::WpsDebug => {
            app.wps_debug = !app.wps_debug;
            app::save_settings(app);
        }
        SetHit::WpsTest => {
            app.wps_test_mode = !app.wps_test_mode;
            app::save_settings(app);
        }
        SetHit::WpsRemove => {
            let (message, ok) = match wps::remove_addin_registration() {
                Ok(text) => (format!("已清除当前用户的 WPS 加载项。\n请重启 WPS 使改动生效。\n{text}"), true),
                Err(error) => (format!("清除 WPS 加载项失败：\n{error}"), false),
            };
            if ok { diag_log::info(&message); } else { diag_log::error(&message); }
            app.diag_text = format!("{message}\n\n日志：{}", diag_log::path()
                .map(|p| p.display().to_string()).unwrap_or_default());
            app.show_diag = true;
        }
        SetHit::ThemeAccent => {
            app.theme.accent = app.theme.accent.next();
            app.apply_theme(app.theme);
            app::save_settings(app);
        }
        SetHit::WpsRegister => {
            let (message, ok) = match wps::register_addin() {
                Ok(text) => (format!("已重新登记 WPS 加载项。\n请重启 WPS 使改动生效。\n{text}"), true),
                Err(error) => (format!("重新登记 WPS 加载项失败：\n{error}"), false),
            };
            if ok { diag_log::info(&message); } else { diag_log::error(&message); }
            app.diag_text = format!("{message}\n\n日志：{}", diag_log::path()
                .map(|p| p.display().to_string()).unwrap_or_default());
            app.show_diag = true;
        }
        SetHit::RightClick => {
            app.right_click_cursor_on = !app.right_click_cursor_on;
            app::save_settings(app);
        }
        SetHit::EraseTrig => {
            app.erase_by_finger = !app.erase_by_finger;
            app.settings_page = ui::settings_layout(app).page;
            app::save_settings(app);
        }
        SetHit::ClearLog => {
            let _ = std::fs::remove_file(wps::log_file());
            diag_log::clear();
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
        SetHit::SettingsPage(delta) => { ui::settings_change_page(app, delta); }
    }
    apply_input_shape(rt, app);
    rt.redraw_full(app);
}

fn first_hit(r: R, x: i32, y: i32) -> bool {
    r.contains(x, y)
}

fn set_sidebar_drag_feedback(rt: &Rt, app: &mut App, feedback: Option<ui::SidebarDragFeedback>) {
    if app.sidebar_drag.is_none() && feedback.is_none() { return; }
    app.sidebar_drag = feedback;
    for right in [false, true] { rt.redraw(app, ui::sidebar_rect(app, right)); }
}

pub(crate) fn on_press(
    rt: &Rt, app: &mut App, ip: &mut Ip, wps: Option<&WpsBridge>,
    x: i32, y: i32, button: u8,
) {
    if app.launch.active() { return; }
    if button == 4 || button == 5 {
        on_scroll(rt, app, ip, x, y, if button == 4 { -1.0 } else { 1.0 });
        return;
    }
    if app.page_picker.as_ref().is_some_and(|p| p.dragging()) { return; }
    // The press that started a drag owns it until release. A second button
    // must not replace it with a pending click and consume that release.
    if ip.pending_ui.is_some() || ip.slider.is_some() || ip.dragging { return; }
    if !app.settings_open && !app.confirm_quit && !app.show_diag
        && ui::sidebar_animating(app) && [false, true].into_iter()
        .any(|right| ui::sidebar_input_rect(app, right).contains(x, y)) { return; }
    if let Some(control) = ui::control_at(app, x, y) {
        if button != 1 || !ui::control_enabled(app, control) { return; }
        if control == ui::Control::PagePicker(ui::PagePickerHit::Wheel) {
            app.press = None;
            if let Some(picker) = app.page_picker.as_mut() { picker.begin(y, Instant::now()); }
            rt.redraw(app, ui::page_picker_rect(app));
            return;
        }
        if let Some(old) = app.press.take() {
            if let Some(r) = ui::control_rect(app, old.control) { rt.redraw(app, r); }
        }
        app.press = Some(ui::PressFeedback::new(control));
        if let Some(r) = ui::control_rect(app, control) { rt.redraw(app, r); }
        if !matches!(control, ui::Control::Slider(_)) {
            ip.pending_ui = Some(control);
            return;
        }
    }
    activate_press(rt, app, ip, wps, x, y, button);
}

// Discrete controls activate on release; drawing and dragging start immediately.
fn activate_press(
    rt: &Rt,
    app: &mut App,
    ip: &mut Ip,
    wps: Option<&WpsBridge>,
    x: i32,
    y: i32,
    button: u8,
) {
    if app.show_input_group_prompt {
        if let Some(h) = ui::input_group_prompt_hit(app, x, y) {
            match h {
                ui::PromptHit::Authorize => {
                    if app.input_group_state == 0 {
                        start_input_group_auth(app);
                    }
                }
                ui::PromptHit::Close => {
                    app.show_input_group_prompt = false;
                    apply_input_shape(rt, app);
                }
            }
            rt.redraw_full(app);
        }
        return;
    }
    if app.confirm_quit {
        if let Some(h) = ui::confirm_hit(app, x, y) {
            match h {
                ui::ConfirmHit::Yes => std::process::exit(0),
                ui::ConfirmHit::No => {
                    app.confirm_quit = false;
                    apply_input_shape(rt, app);
                    rt.redraw_full(app);
                }
            }
        }
        return;
    }
    if app.show_diag {
        if ui::diag_close_hit(app, x, y) {
            app.show_diag = false;
            apply_input_shape(rt, app);
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
    if app.page_picker.is_some() {
        if ui::page_picker_rect(app).contains(x, y) {
            if button == 1 {
                match ui::page_picker_hit(app, x, y) {
                    Some(ui::PagePickerHit::Cancel) => close_all_popups(rt, app),
                    Some(ui::PagePickerHit::Jump) => commit_page_picker(rt, app, wps),
                    _ => (),
                }
            }
            return;
        }
        if let Some(right) = ui::page_count_at(app, x, y) {
            let same_side = app.popup_on_right == right;
            close_all_popups(rt, app);
            if !same_side { open_page_picker(rt, app, right); }
            return;
        }
        close_all_popups(rt, app);
    }
    if let Some(right) = ui::page_count_at(app, x, y) {
        if button == 1 { open_page_picker(rt, app, right); }
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
        apply_input_shape(rt, app);
        rt.redraw_full(app);
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
                PopupHit::ClearAll => clear_drawing(rt, app),
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
            // UI releases are left-button only, so right/wheel presses must
            // never start a drag that their matching release cannot finish.
            if button != 1 { return; }
            log::debug!("[INPUT] sidebar bg drag right={}", right);
            close_all_popups(rt, app);
            app.press = None;
            ip.dragging = true;
            ip.moved = false;
            app.sb_y = sb.y;
            ip.drag_start_y = app.sb_y;
            ip.drag_mouse_y0 = y;
            ip.collapse_press = app.collapsed;
            set_sidebar_drag_feedback(rt, app, Some(ui::SidebarDragFeedback {
                right,
            }));
            return;
        }
    }

    log::debug!("[INPUT] 未命中 UI (mode={})", app.mode);
    // 画布区域
    if app.mode != 0 {
        if button == 3 {
            if !app.whiteboard && app.right_click_cursor_on && app.mode == 1 {
                switch_to_cursor(rt, app);
                if rt.backend.can_forward_right_click() { ip.pending_right_click = Some((x, y)); }
            }
            return;
        }
        // 落在弹窗/菜单等 UI 上：不落笔
        if over_ui(app, x, y) {
            return;
        }
        close_all_popups(rt, app);
        app.begin_undo();
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
            // 增量落笔：直接画圆点到画布，不再攒 stroke_pts 每帧重建整条
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

pub(crate) fn clear_drawing(rt: &Rt, app: &mut App) {
    close_all_popups(rt, app);
    // Clearing desktop annotations ends drawing; a whiteboard remains ready to write.
    app.remember_before_clear();
    app.is_drawing = false;
    app.reset_palm_gesture();
    app.clear_current_strokes();
    app.mode = if app.whiteboard { 1 } else { 0 };
    // Keep the tool group visible so the new restore action is reachable.
    app.drawing_tools.set_expanded(app.whiteboard || app.can_restore_cleared());
    apply_input_shape(rt, app);
    rt.redraw_full(app);
}

pub(crate) fn handle_btn(rt: &Rt, app: &mut App, wps: Option<&WpsBridge>, k: Btn, right: bool) {
    if !ui::control_enabled(app, ui::Control::Sidebar(k, right)) { return; }
    match k {
        Btn::Collapse => toggle_collapse(rt, app),
        Btn::Cursor => switch_to_cursor(rt, app),
        Btn::Pen => {
            if app.mode == 1 && app.drawing_tools.expanded() {
                let was = app.pen_popup_visible;
                close_all_popups(rt, app);
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
            if app.mode == 2 && app.drawing_tools.expanded() {
                let was = app.eraser_popup_visible;
                close_all_popups(rt, app);
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
        Btn::Clear => clear_drawing(rt, app),
        Btn::Restore => {
            if app.restore_cleared() {
                rt.redraw_full(app);
                apply_input_shape(rt, app);
            }
        }
        Btn::Undo => {
            app.undo_last();
            rt.redraw_full(app);
        }
        Btn::Prev => go_prev(rt, app, wps),
        Btn::Next => go_next(rt, app, wps),
        Btn::Exit => exit_presentation(rt, app, wps),
        Btn::Whiteboard => {
            close_all_popups(rt, app);
            app.toggle_whiteboard();
            if let Some(w) = wps { w.set_whiteboard(app.whiteboard); }
            apply_input_shape(rt, app);
            rt.redraw_full(app);
        }
        Btn::More => {
            close_all_popups(rt, app);
            app.more_menu_open = true;
            app.open_anim = now_ms();
            apply_input_shape(rt, app);
            rt.redraw_full(app);
        }
    }
    let _ = right;
}

pub(crate) fn on_motion(rt: &Rt, app: &mut App, ip: &mut Ip, x: i32, y: i32) {
    if app.page_picker.as_ref().is_some_and(|p| p.dragging()) {
        if let Some(picker) = app.page_picker.as_mut() { picker.motion(y, Instant::now()); }
        rt.redraw(app, ui::page_picker_rect(app));
        return;
    }
    // Moving an idle mouse has no visual effect. Only an active press reacts.
    if let Some(control) = ip.pending_ui {
        let active = ui::control_enabled(app, control) && ui::control_at(app, x, y) == Some(control);
        if let Some(press) = app.press.as_mut() {
            if press.active != active {
                press.active = active;
                if let Some(r) = ui::control_rect(app, control) { rt.redraw(app, r); }
            }
        }
        return;
    }
    // 设置面板滑条拖动：只重绘设置面板区域（约 5ms，而非整屏 ~58ms）
    if let Some(id) = ip.slider {
        if let Some(def) = ui::settings_sliders(app).into_iter().find(|s| s.id == id) {
            let old_sidebars = [ui::sidebar_input_rect(app, false), ui::sidebar_input_rect(app, true)];
            apply_slider(app, id, def.value_at(x));
            rt.redraw(app, ui::settings_rect(app));
            // 透明度/大小会改变侧栏本身，需一并重绘两侧栏
            if matches!(id, ui::SliderId::Alpha | ui::SliderId::Scale) {
                for old in old_sidebars { rt.redraw(app, old); }
                for right in [false, true] {
                    rt.redraw(app, ui::sidebar_rect(app, right));
                }
            }
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
        let old_y = app.sidebar_y(h);
        let new_y = (ip.drag_start_y + dy).clamp(0, (app.screen_h - h).max(0));
        // Rebase at each sample, including a clamped edge. Reversing direction
        // must move immediately rather than requiring the pointer to retrace
        // the distance it travelled past the screen boundary.
        ip.drag_start_y = new_y;
        ip.drag_mouse_y0 = y;
        app.sb_y = new_y;
        if new_y == old_y {
            return;
        }
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
        // Keep the original input target alive until release. X11/layer-shell
        // retain their implicit grab; GNOME holds an explicit Clutter grab.
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
        // 刚从 UI 出来：从当前点新起一段（避免跨侧栏拉一条线）
        ip.stroke_skipped = false;
        app.last_pen_w = w;
        app.last_pt = cur;
        if erase {
            paint::stroke_segment(app, cur, cur, true, w);
        } else {
            paint::stroke_tapered(app, cur, cur, w, w);
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
        // 增量绘制本段（前一宽度→当前宽度），O(1)，不再重建整条笔迹
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

pub(crate) fn on_release(rt: &Rt, app: &mut App, ip: &mut Ip, wps: Option<&WpsBridge>, x: i32, y: i32, button: u8) {
    if button == 4 || button == 5 { return; }
    if app.page_picker.as_ref().is_some_and(|p| p.dragging()) {
        if button != 1 { return; }
        let r = ui::page_picker_control_rect(app, ui::PagePickerHit::Wheel);
        if let Some(picker) = app.page_picker.as_mut() { picker.release(y, r.y + r.h / 2, Instant::now()); }
        rt.redraw(app, ui::page_picker_rect(app));
        return;
    }
    if button != 1 && (ip.pending_ui.is_some() || ip.slider.is_some() || ip.dragging) { return; }
    if let Some(control) = ip.pending_ui.take() {
        let activate = ui::control_enabled(app, control) && ui::control_at(app, x, y) == Some(control);
        if let Some(press) = app.press.as_mut() {
            press.active = activate;
            press.release();
        }
        if activate { activate_press(rt, app, ip, wps, x, y, 1); }
        if let Some(r) = ui::control_rect(app, control) { rt.redraw(app, r); }
        return;
    }
    if button == 3 {
        if let Some((x, y)) = ip.pending_right_click.take() {
            rt.backend.flush();
            rt.virtual_right_click(x, y);
        }
    }
    if ip.slider.is_some() {
        ip.slider = None;
        if let Some(press) = app.press.as_mut() { press.release(); }
        app::save_settings(app);
        apply_input_shape(rt, app);
        rt.redraw_full(app);
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
        // Include the release position even if its final motion was coalesced.
        on_motion(rt, app, ip, x, y);
        ip.dragging = false;
        set_sidebar_drag_feedback(rt, app, None);
        if ip.collapse_press && !ip.moved {
            toggle_collapse(rt, app);
        } else if ip.moved {
            // 拖动结束：按新位置更新输入区域
            apply_input_shape(rt, app);
        }
    }
}

/// 触摸事件（X11 与 Wayland 共用）
pub(crate) fn cancel_input(rt: &Rt, app: &mut App, ip: &mut Ip) {
    if let Some(picker) = app.page_picker.as_mut() { picker.cancel_drag(); }
    if app.is_drawing { paint::commit_stroke(app); }
    app.is_drawing = false;
    app.reset_palm_gesture();
    app.touch_frame_contacts = 0;
    app.press = None;
    app.sidebar_drag = None;
    *ip = Ip::default();
    apply_input_shape(rt, app);
    rt.redraw_full(app);
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
    ip.last_touch_ms = now_ms();
    if kind != TouchKind::End {
        ip.last_touch_pos = (x, y);
    }
    if app.launch.active() {
        // A finger held through the intro remains ignored until its own End;
        // it must not become a drawing stroke when the toolbar becomes ready.
        match kind {
            TouchKind::Begin => { ip.touch_ids.insert(id); ip.touch_over_ui.insert(id, true); }
            TouchKind::End => { ip.touch_ids.remove(&id); ip.touch_over_ui.remove(&id); ip.touch_shape.remove(&id); }
            TouchKind::Update => (),
        }
        return;
    }
    let wants_touch = app.launch.active() || app.whiteboard || app.mode != 0
        || app.settings_open || app.confirm_quit || app.show_diag || app.show_input_group_prompt
        || over_ui(app, x, y);
    match kind {
        TouchKind::Begin => {
            if !wants_touch { return; }
            ip.touch_ids.insert(id);
            if over_ui(app, x, y) || ip.ui_touch.is_some() {
                ip.touch_over_ui.insert(id, true);
                // 先清掉陈旧拖动状态，再交给 on_press：命中侧栏空白会置 dragging=true，
                // 命中按钮/滑条则不会，从而保留触屏拖动侧边栏的能力。
                // Only the first finger owns a control; other fingers cannot
                // toggle it twice or release somebody else's slider.
                if ip.touch_ids.len() == 1 && ip.pending_ui.is_none() && !app.is_drawing {
                    ip.ui_touch = Some(id);
                    ip.dragging = false;
                    set_sidebar_drag_feedback(rt, app, None);
                    on_press(rt, app, ip, wps, x, y, 1);
                }
            } else {
                ip.touch_over_ui.insert(id, false);
                close_all_popups(rt, app);
                if let Some(_r) = gesture::touch_event(app, id, x, y, TouchKind::Begin, diameter) {
                    rt.redraw_full(app);
                }
            }
        }
        TouchKind::Update => {
            if ip.rejected_touch.contains(&id) { return; }
            if !ip.touch_ids.contains(&id) { return; }
            if ip.ui_touch == Some(id) {
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
            if ip.rejected_touch.remove(&id) { return; }
            if !ip.touch_ids.contains(&id) { return; }
            ip.touch_ids.remove(&id);
            let over = ip.touch_over_ui.remove(&id).unwrap_or(false);
            if over {
                if ip.ui_touch == Some(id) {
                    ip.ui_touch = None;
                    on_release(rt, app, ip, wps, x, y, 1);
                }
                return;
            }
            if let Some(r) = gesture::touch_event(app, id, x, y, TouchKind::End, 0.0) {
                rt.redraw(app, r);
            }
        }
    }
}

/// 触摸事件的物理设备号：XInput2 里 deviceid 可能是 master，触点尺寸轴在 slave(sourceid)
#[inline]
fn touch_dev(deviceid: u16, sourceid: u16) -> u8 {
    (if sourceid != 0 { sourceid } else { deviceid }) as u8
}

/// X11 事件坐标是物理像素，需换算成逻辑像素；Wayland 事件本就是逻辑坐标（比例为 1）
#[inline]
fn to_logical(rt: &Rt, v: i32) -> i32 {
    let s = rt.backend.device_pixel_ratio();
    if s <= 0.0 {
        v
    } else {
        (v as f64 / s).round() as i32
    }
}

fn handle_event(rt: &Rt, app: &mut App, ip: &mut Ip, wps: Option<&WpsBridge>, ev: Event) {
    // 屏蔽 X 合成的鼠标事件，避免一次触摸被处理两遍 / 残留笔画
    if let Event::ButtonPress(_) | Event::ButtonRelease(_) | Event::MotionNotify(_) = &ev {
        // 有触摸按下中，或 evdev 触摸启用后（无真实指针则全屏蔽，否则触摸后 400ms 内屏蔽）
        if !ip.touch_ids.is_empty()
            || (ip.evdev_touch
                && (ip.ignore_core_pointer || now_ms() - ip.last_touch_ms < 400))
        {
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
            on_press(
                rt,
                app,
                ip,
                wps,
                to_logical(rt, e.event_x as i32),
                to_logical(rt, e.event_y as i32),
                e.detail,
            )
        }
        Event::MotionNotify(e) => {
            if std::env::var("SIDERA_TRACE").is_ok() {
                log::info!("[MOTION] ({},{})", e.event_x, e.event_y);
            }
            on_motion(
                rt,
                app,
                ip,
                to_logical(rt, e.event_x as i32),
                to_logical(rt, e.event_y as i32),
            )
        }
        Event::ButtonRelease(e) => on_release(
            rt,
            app,
            ip,
            wps,
            to_logical(rt, e.event_x as i32),
            to_logical(rt, e.event_y as i32),
            e.detail,
        ),
        Event::LeaveNotify(_) => {
            if ip.pending_ui.is_some() { on_motion(rt, app, ip, -1, -1); }
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
                let d = xp.touch_diameter(touch_dev(e.deviceid, e.sourceid), &e.valuator_mask, &e.axisvalues_raw);
                if d > 0.0 {
                    ip.touch_shape.insert(e.detail as i32, d);
                }
            }
        }
        Event::XinputRawTouchEnd(e) => {
            ip.touch_shape.remove(&(e.detail as i32));
        }
        Event::XinputTouchBegin(e) => {
            let x = to_logical(rt, (e.event_x as f64 / 65536.0).round() as i32);
            let y = to_logical(rt, (e.event_y as f64 / 65536.0).round() as i32);
            let d = ip.touch_shape.get(&(e.detail as i32)).copied().unwrap_or_else(|| {
                rt.backend
                    .as_any()
                    .downcast_ref::<x11::X11Plat>()
                    .map(|xp| xp.touch_diameter(touch_dev(e.deviceid, e.sourceid), &e.valuator_mask, &e.axisvalues))
                    .unwrap_or(0.0)
            });
            let id = e.detail as i32;
            let wants_touch = app.launch.active() || app.whiteboard || app.mode != 0
                || app.settings_open || app.confirm_quit || app.show_diag || app.show_input_group_prompt
                || over_ui(app, x, y);
            log::info!("[TOUCH] begin id={} ({},{}) d={:.1} mode={} want={}",
                e.detail, x, y, d, app.mode, wants_touch);
            if let Some(xp) = rt.backend.as_any().downcast_ref::<x11::X11Plat>() {
                // Cursor mode must stay click-through: rejecting ownership lets
                // WPS receive the touch instead of the full-screen overlay.
                xp.x11.allow_touch(e.deviceid, e.detail, e.event, wants_touch);
            }
            if wants_touch {
                handle_touch(rt, app, ip, wps, id, x, y, TouchKind::Begin, d);
            } else {
                if ip.rejected_touch.len() > 64 { ip.rejected_touch.clear(); }
                ip.rejected_touch.insert(id);
                use std::sync::atomic::{AtomicI64, Ordering};
                static LAST: AtomicI64 = AtomicI64::new(0);
                let now = now_ms();
                if now - LAST.load(Ordering::Relaxed) > 2000 {
                    LAST.store(now, Ordering::Relaxed);
                    diag_log::info(&format!("光标模式触摸穿透：id={id} ({x},{y})"));
                }
            }
        }
        Event::XinputTouchUpdate(e) => {
            let id = e.detail as i32;
            if ip.rejected_touch.contains(&id) { return; }
            let x = to_logical(rt, (e.event_x as f64 / 65536.0).round() as i32);
            let y = to_logical(rt, (e.event_y as f64 / 65536.0).round() as i32);
            let d = ip.touch_shape.get(&id).copied().unwrap_or_else(|| {
                rt.backend
                    .as_any()
                    .downcast_ref::<x11::X11Plat>()
                    .map(|xp| xp.touch_diameter(touch_dev(e.deviceid, e.sourceid), &e.valuator_mask, &e.axisvalues))
                    .unwrap_or(0.0)
            });
            handle_touch(rt, app, ip, wps, id, x, y, TouchKind::Update, d);
        }
        Event::XinputTouchEnd(e) => {
            let id = e.detail as i32;
            if ip.rejected_touch.remove(&id) { return; }
            let x = to_logical(rt, (e.event_x as f64 / 65536.0).round() as i32);
            let y = to_logical(rt, (e.event_y as f64 / 65536.0).round() as i32);
            handle_touch(rt, app, ip, wps, id, x, y, TouchKind::End, 0.0);
        }
        Event::XinputTouchOwnership(e) => {
            let accept = !ip.rejected_touch.contains(&(e.touchid as i32));
            if let Some(xp) = rt.backend.as_any().downcast_ref::<x11::X11Plat>() {
                xp.x11.allow_touch(e.deviceid, e.touchid, e.event, accept);
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

fn single_instance() -> Result<Option<UnixListener>, Box<dyn std::error::Error>> {
    let command: &[u8] = if std::env::args().any(|arg| arg == "--toggle") { b"toggle" } else { b"show" };
    let instance = instance::acquire(command).map_err(|error| format!("无法创建单实例通信文件：{error}"))?;
    if instance.is_none() { log::warn!("Sidera 已在运行，退出新实例"); }
    Ok(instance)
}

pub(crate) struct Timers {
    last_wps: Instant,
    last_foreground: Instant,
    foreground: Option<bool>,
    foreground_false: u8,
}

impl Timers {
    pub(crate) fn new() -> Self {
        Timers {
            last_wps: Instant::now() - Duration::from_secs(10),
            last_foreground: Instant::now() - Duration::from_secs(10),
            foreground: None,
            foreground_false: 0,
        }
    }
}

/// 与具体后端无关的周期性任务：WPS 桥/事件/心跳/全屏检测、白板提示、侧栏动画
pub(crate) fn tick(
    rt: &Rt,
    app: &mut App,
    ip: &mut Ip,
    wps_bridge: &mut Option<WpsBridge>,
    t: &mut Timers,
) {
    let update = poll_wps(app.wps_debug, app.whiteboard, app.wps_test_mode, wps_bridge);
    tick_shared(rt, app, ip, wps_bridge.as_ref(), &update, t);
    resolve_drawing_tools_auto_update(rt, app);
}

pub(crate) fn set_drawing_tools_expanded(rt: &Rt, app: &mut App, expanded: bool) {
    if app.drawing_tools.expanded() == expanded { return; }
    close_all_popups(rt, app);
    app.drawing_tools.set_expanded(expanded);
    apply_input_shape(rt, app);
    rt.redraw_full(app);
}

pub(crate) fn resolve_drawing_tools_auto_update(rt: &Rt, app: &mut App) {
    let Some(update) = app.drawing_tools.take_auto_update() else { return };
    let Some(has_ink) = app.drawing_tools_auto_state() else { return };
    if let Some(expanded) = update.expanded_for(has_ink) {
        set_drawing_tools_expanded(rt, app, expanded);
    }
    // input 组授权结果同步到界面
    let igs = INPUT_GROUP_STATE.load(std::sync::atomic::Ordering::Relaxed);
    if igs != app.input_group_state {
        app.input_group_state = igs;
        if app.show_input_group_prompt {
            rt.redraw(app, ui::input_group_prompt_rect(app));
        }
    }
}

pub(crate) struct WpsUpdate {
    connected: bool,
    events: Vec<WpsEvent>,
}

pub(crate) fn begin_launch(rt: &Rt, app: &mut App) -> bool {
    if std::env::var_os("SIDERA_NO_SPLASH").is_some() { return false; }
    app.launch = launch::Launch::begin(
        format!("{} · {} × {}", rt.backend.description(), app.screen_w, app.screen_h),
        !rt.fonts.list.is_empty() && rt.icon_big.is_some(), app.hotkey_ok, Instant::now());
    true
}

fn tick_launch(rt: &Rt, app: &mut App, bridge: Option<&WpsBridge>, connected: bool) {
    if !app.launch.active() { return; }
    let now = Instant::now();
    let old_alpha = app.launch.panel_alpha();
    let old_reveal = app.launch.reveal();
    let link = if !app.wps_debug { launch::LinkState::Disabled }
        else if connected { launch::LinkState::Connected }
        else if bridge.is_some() { launch::LinkState::Waiting }
        else { launch::LinkState::Unavailable };
    // poll_wps has completed a real service setup attempt before this call.
    let report_changed = app.launch.report_link(link, now);
    app.launch.advance_at(now);
    if report_changed || old_alpha != app.launch.panel_alpha() { rt.redraw(app, ui::launch_rect(app)); }
    if old_reveal != app.launch.reveal() {
        for right in [false, true] { rt.redraw(app, ui::sidebar_transition_rect(app, right)); }
    }
    if (old_alpha > 0.0) != (app.launch.panel_alpha() > 0.0)
        || (old_reveal > 0.0) != (app.launch.reveal() > 0.0) || !app.launch.active() {
        apply_input_shape(rt, app);
    }
}

pub(crate) fn poll_wps(enabled: bool, whiteboard: bool, test_mode: bool, wps_bridge: &mut Option<WpsBridge>) -> WpsUpdate {
    // 静止触点没有 Motion 事件，不按闲置时长猜测已经抬手。
    // 取消、设备移除和输出移除由后端明确调用 cancel_input。
    // WPS 桥存在性与设置同步
    if enabled && wps_bridge.is_none() {
        *wps_bridge = WpsBridge::start();
    } else if !enabled && wps_bridge.is_some() {
        *wps_bridge = None;
    }
    if let Some(w) = wps_bridge.as_ref() {
        w.set_whiteboard(whiteboard);
        w.set_test_mode(test_mode);
        w.tick();
        WpsUpdate { connected: w.connected(), events: w.take_events() }
    } else {
        WpsUpdate { connected: false, events: Vec::new() }
    }
}

pub(crate) fn tick_shared(rt: &Rt, app: &mut App, _ip: &mut Ip,
    wps_bridge: Option<&WpsBridge>, update: &WpsUpdate, t: &mut Timers) {
    if app.update_system_theme(system_theme::dark()) { rt.redraw_full(app); }
    tick_launch(rt, app, wps_bridge, update.connected);
    if app.sidebar_expand_due() { expand_sidebars(rt, app); }
    // Sample foreground at a low rate. Doing this every 4ms caused hundreds of
    // X11 round trips per second and made the page/mode state flutter on slow
    // classroom machines. Require two consecutive "not foreground" samples so
    // a transient focus change cannot leave presentation mode.
    let had_foreground_sample = t.foreground.is_some();
    let was_visible = match t.foreground {
        Some(false) => had_foreground_sample && t.foreground_false < 2,
        _ => true,
    };
    if t.last_foreground.elapsed() >= Duration::from_millis(500) {
        t.last_foreground = Instant::now();
        let sample = rt.wps_foreground();
        if sample == Some(false) { t.foreground_false = t.foreground_false.saturating_add(1); }
        else { t.foreground_false = 0; }
        t.foreground = sample;
        let now_visible = match t.foreground {
            Some(false) => had_foreground_sample && t.foreground_false < 2,
            _ => true,
        };
        if now_visible != was_visible {
            if now_visible { diag_log::info("WPS/覆盖层进入前台，恢复放映 UI"); }
            else { diag_log::warn("WPS/覆盖层离开前台，隐藏放映 UI"); }
        }
    }
    let wps_visible = match t.foreground {
        Some(false) => had_foreground_sample && t.foreground_false < 2,
        _ => true,
    };
    let jump_epoch = if app.wps_debug && wps_visible { wps_bridge.and_then(WpsBridge::jump_epoch) } else { None };
    let jump_changed = app.wps_can_jump != jump_epoch.is_some() || app.wps_deck_epoch != jump_epoch.unwrap_or(-1);
    app.wps_can_jump = jump_epoch.is_some();
    app.wps_deck_epoch = jump_epoch.unwrap_or(-1);
    let pages = if app.wps_debug && wps_visible {
        wps_bridge.map(WpsBridge::page_progress).unwrap_or((-1, -1))
    } else { (-1, -1) };
    let show_pages = app.whiteboard || (app.wps_debug && update.connected && (pages.0 > 0 || pages.1 > 0));
    let layout_changed = app.sidebar_page_visible() != show_pages;
    let old_sidebars = [ui::sidebar_input_rect(app, false), ui::sidebar_input_rect(app, true)];
    // Popups follow toolbar geometry. Close them using their old bounds before
    // changing the layout so no stale panel pixels or hit regions remain.
    if layout_changed { close_all_popups(rt, app); }
    if app.wps_connected != update.connected {
        app.wps_connected = update.connected;
        if app.settings_open { rt.redraw(app, ui::settings_rect(app)); }
    }

    if (app.wps_page_index, app.wps_page_total) != pages {
        (app.wps_page_index, app.wps_page_total) = pages;
        if !layout_changed && show_pages && !app.collapsed {
            for right in [false, true] { rt.redraw(app, ui::sidebar_page_rect(app, right)); }
        }
    }
    if jump_changed && app.sidebar_page_visible() && !app.collapsed {
        for right in [false, true] { rt.redraw(app, ui::sidebar_page_rect(app, right)); }
    }
    if app.page_picker.as_ref().is_some_and(|picker| {
        app.page_picker_context().is_none_or(|(_, total, source)| total != picker.total || source != picker.source)
    }) { close_all_popups(rt, app); }
    if app.page_picker.as_mut().is_some_and(|p| p.advance_at(Instant::now())) {
        rt.redraw(app, ui::page_picker_rect(app));
    }
    if layout_changed {
        for (i, right) in [false, true].into_iter().enumerate() {
            rt.redraw(app, old_sidebars[i]);
            rt.redraw(app, ui::sidebar_input_rect(app, right));
        }
        apply_input_shape(rt, app);
    }

    // WPS 事件（真实页号 / 放映开始）
    // 后台放映的开始/翻页事件不切换当前画布；结束事件仍处理，避免残留放映状态。
    {
        for ev in update.events.iter().cloned() {
            if !wps_visible && !matches!(ev, WpsEvent::SlideshowEnd) {
                continue;
            }
            match ev {
                WpsEvent::SlideshowBegin(pos) => {
                    app.clear_slide_pages();
                    app.wps_real_pos = pos;
                    if app.whiteboard {
                        app.saved_slide = pos;
                    } else {
                        app.current_slide = pos;
                    }
                    rt.redraw_full(app);
                }
                WpsEvent::SlideshowEnd => {
                    app.clear_slide_pages();
                    app.wps_real_pos = -1;
                    rt.redraw_full(app);
                }
                WpsEvent::RealPos(pos) => {
                    if app.whiteboard {
                        app.saved_slide = pos;
                        app.wps_real_pos = pos;
                        continue;
                    }
                    if app.current_slide == pos && app.wps_real_pos == pos { continue; }
                    if app.wps_real_pos > 0 {
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



    // 全屏检测（1s；始终扫描，避免加载项连着但已退出放映时状态变陈旧）
    if t.last_wps.elapsed() >= Duration::from_millis(1000) {
        t.last_wps = Instant::now();
        let fs = rt.is_fullscreen();
        if fs != app.wps_fullscreen {
            app.wps_fullscreen = fs;
            if !app.whiteboard && !app.wps_debug {
                app.clear_slide_pages();
            }
            rt.redraw_full(app);
        }
        if app.mode == 0 && !app.settings_open {
            apply_input_shape(rt, app);
        }
    }

    let linked_show = app.wps_debug && wps_visible && wps_bridge.is_some_and(|w| w.showing());
    // Reconnecting to the same reported slide need not emit a new WPS event.
    // Reconcile the locally estimated page without filing its ink under an old page.
    if app.wps_debug && wps_visible && !app.whiteboard {
        if let Some(pos) = wps_bridge.and_then(|w| w.position()) {
            if app.current_slide != pos {
                app.save_current_page();
                app.current_slide = pos;
                app.load_page(pos);
                rt.redraw_full(app);
            }
            app.wps_real_pos = pos;
        }
    }
    let navigation_ready = navigation_route(linked_show, wps_bridge.is_some_and(|w| w.ready()),
        rt.backend.can_inject_keys_without_prompt()) != NavigationRoute::Unavailable;
    let action = if linked_show {
        ui::PresentationAction::End
    } else if app.wps_debug && wps_visible && (
        (!app.wps_test_mode && update.connected)
            || (app.wps_test_mode && wps_bridge.is_some_and(|w| w.start_available()))
    ) {
        ui::PresentationAction::Start
    } else {
        ui::PresentationAction::Escape
    };
    if app.wps_navigation_ready != navigation_ready {
        app.wps_navigation_ready = navigation_ready;
        rt.redraw_full(app);
    }
    app.presentation_action = action;
    let presentation_frame = Instant::now();
    app.presentation_transition.set_action_at(action, presentation_frame);
    if app.presentation_transition.advance_at(presentation_frame)
        && !app.whiteboard && !ui::sidebar_animating(app) {
        // This visual transition does not change geometry, input regions or
        // click availability. Moving sidebars already repaint their full strip.
        for right in [false, true] {
            if let Some(r) = ui::control_rect(app, ui::Control::Sidebar(Btn::Exit, right)) {
                rt.redraw(app, r);
            }
        }
    }

    if app.press.is_some_and(|p| p.expired()) {
        let old = app.press.take().unwrap();
        if let Some(r) = ui::control_rect(app, old.control) { rt.redraw(app, r); }
    }

    // 白板上限提示到点重绘
    if app.wb_limit_msg_until != 0 && now_ms() > app.wb_limit_msg_until {
        app.wb_limit_msg_until = 0;
        rt.redraw_full(app);
    }
    if app.log_cleared_until != 0 && now_ms() > app.log_cleared_until {
        app.log_cleared_until = 0;
        if app.settings_open {
            rt.redraw(app, ui::settings_rect(app));
        }
    }

    if ui::sidebar_animating(app) {
        app.drawing_tools.advance_at(Instant::now());
        app.finish_sidebar_animation();
        for right in [false, true] { rt.redraw(app, ui::sidebar_transition_rect(app, right)); }
        if !ui::sidebar_animating(app) { apply_input_shape(rt, app); }
    }

    // 弹窗/菜单淡入动画期间持续重绘（只重绘对应弹窗区域）
    if app.open_anim != 0 && now_ms() < app.open_anim + 180 {
        if app.page_picker.is_some() { rt.redraw(app, ui::page_picker_rect(app)); }
        if app.pen_popup_visible {
            rt.redraw(app, ui::pen_popup_rect(app));
        }
        if app.eraser_popup_visible {
            rt.redraw(app, ui::eraser_popup_rect(app));
        }
        if app.more_menu_open {
            rt.redraw(app, ui::more_menu_rect(app));
        }
    }
}

fn main() -> std::process::ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    match run_application() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            startup::report_error(&error);
            std::process::ExitCode::FAILURE
        }
    }
}

fn run_application() -> Result<(), Box<dyn std::error::Error>> {
    // 软件渲染（无 GPU 老机器）
    std::env::set_var("LIBGL_ALWAYS_SOFTWARE", "1");

    let backend = startup::detect_backend()?;
    diag_log::info(&format!("Sidera {} 启动，后端：{:?}", app::VERSION, backend));

    let single = single_instance()?;
    if single.is_none() {
        return Ok(());
    }

    system_theme::start();
    let fonts = Fonts::load();
    let icon_big = load_icon(84);
    let icon = load_icon(44);
    if icon.is_some() {
        log::info!("[INFO] 已加载软件图标");
    } else {
        log::warn!("[WARN] 未找到 sidera.png，设置/闪屏将无图标（可设 SIDERA_ICON 指定）");
    }

    let gnome_desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default()
        .split(':').any(|name| name.eq_ignore_ascii_case("gnome"));
    if backend == startup::DisplayBackend::Gnome || (backend == startup::DisplayBackend::Wayland && gnome_desktop) {
        if let Some(prepared) = gnome::prepare()? {
            log::info!("[INFO] 平台: GNOME 原生 Wayland");
            return gnome::run(prepared, fonts, icon, icon_big, single);
        }
        if backend == startup::DisplayBackend::Gnome {
            return Err("GNOME 原生扩展尚未启用。请运行 sidera-gnome-install，重新登录后启用 Sidera 原生批注扩展。也可通过 SIDERA_BACKEND=x11 使用兼容模式。".into());
        }
    }
    if backend == startup::DisplayBackend::Wayland {
        match wayland::prepare() {
            Ok(prepared) => {
                log::info!("[INFO] 平台: wayland");
                return wayland::run(prepared, fonts, icon, icon_big, single);
            }
            Err(wayland::PrepareError::MissingLayerShell) if startup::allow_x11_fallback() => {
                log::warn!("[WARN] 当前桌面没有 layer-shell，使用 X11/XWayland 兼容模式；全局热键、原生 Wayland 应用的翻页/截图等能力可能受桌面限制");
            }
            Err(error) => return Err(error.into()),
        }
    }

    let x11 = x11::X11::connect().map_err(|e| format!("无法连接 X11 桌面：{e}"))?;
    let scale = x11.detect_scale();
    let logical_w = ((x11.width as f64) / scale).round().max(1.0) as i32;
    let logical_h = ((x11.height as f64) / scale).round().max(1.0) as i32;
    log::info!(
        "[INFO] 平台: x11  屏幕 {}x{} (物理 {}x{})  缩放 {}",
        logical_w,
        logical_h,
        x11.width,
        x11.height,
        scale
    );

    let (win, gc) = x11.create_overlay().map_err(|e| format!("创建 X11 覆盖层失败：{e}"))?;

    // 触摸输入：先协商 XInput 2.2 并优先用 XInput2 触摸；不可用再退回 evdev。
    // 关键：不调用 XIQueryVersion(2,2) 时服务器按 2.0 处理，不会投递触摸事件。
    let (xi_major, xi_minor) = match x11.negotiate_xi() {
        Ok(v) => v,
        Err(e) => {
            log::warn!("[WARN] XIQueryVersion 失败: {}", e);
            (0, 0)
        }
    };
    let xi22 = xi_major > 2 || (xi_major == 2 && xi_minor >= 2);
    let touch_ids = if xi22 { x11.touch_device_ids() } else { Vec::new() };
    log::info!(
        "[TOUCH] XInput {}.{}，触摸设备 {} 个",
        xi_major,
        xi_minor,
        touch_ids.len()
    );
    let mut evdev_touch = None;
    let mut xi_touch = false;
    if !touch_ids.is_empty() {
        match x11.select_touch_events(win) {
            Ok(()) => {
                xi_touch = true;
                log::info!("[TOUCH] 使用 XInput2 触摸");
            }
            Err(e) => log::warn!("[WARN] XInput2 触摸选择失败: {}", e),
        }
    }
    if !xi_touch {
        evdev_touch = touch_evdev::spawn(logical_w, logical_h);
        if evdev_touch.is_some() {
            log::info!("[TOUCH] 使用 evdev 内核触摸输入");
        }
    }

    let has_node = touch_evdev::has_touch_node();
    let in_group = touch_evdev::in_input_group();
    let need_input_group = !xi_touch && evdev_touch.is_none() && has_node && !in_group;
    log::info!(
        "[PERM] xi_touch={} evdev_started={} has_touch_node={} in_input_group={} -> prompt={}",
        xi_touch,
        evdev_touch.is_some(),
        has_node,
        in_group,
        need_input_group
    );

    let mut canvas = Pixmap::new(x11.width as u32, x11.height as u32).unwrap();
    canvas.fill(tiny_skia::Color::TRANSPARENT);
    let mut app = App::new(canvas, logical_w, logical_h);
    app::load_settings(&mut app);
    app.update_system_theme(system_theme::dark());
    // X11 DPI 缩放：逻辑尺寸 × scale = 物理分辨率画布
    app.set_scale(scale);
    app.show_input_group_prompt = need_input_group;
    // 环境变量仅本次运行覆盖（不落盘），与 C++ 版一致
    if let Ok(v) = std::env::var("WPS_API_DEBUG") {
        app.wps_debug = v == "1";
    }

    let rt = Rt {
        backend: Box::new(x11::X11Plat::new(x11, win, gc)
            .map_err(|e| format!("初始化 X11 覆盖层失败：{e}"))?),
        fonts,
        icon,
        icon_big,
        batching: std::cell::Cell::new(false),
        dirty: std::cell::RefCell::new(Vec::new()),
    };
    rt.backend.set_scale(scale);

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
        return Ok(());
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
        return Ok(());
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
        return Ok(());
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
        return Ok(());
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
        return Ok(());
    }

    // Shared, non-blocking startup card; the normal event loop drives its exit.
    begin_launch(&rt, &mut app);

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
    ip.evdev_touch = evdev_touch.is_some();
    ip.ignore_core_pointer = evdev_touch.is_some() && !touch_evdev::has_relative_pointer();
    ip.last_touch_ms = now_ms();
    if ip.evdev_touch {
        log::info!(
            "[TOUCH] evdev 触摸已启用，ignore_core_pointer={}",
            ip.ignore_core_pointer
        );
    }

    // 独立追踪真实 Begin/End，不使用手势层 1.5 秒超时清理后的状态
    // 来判断是否可以重映射；静止长按仍可能持有触摸抓取。
    let mut active_x11_touches = HashSet::new();


    let mut timers = Timers::new();

    loop {
        // 本帧开始累积脏区，事件处理期间不立即渲染
        rt.begin_batch();
        // 0. evdev 触摸（若有）
        if let Some(rx) = evdev_touch.as_ref() {
            while let Ok(contacts) = rx.try_recv() {
                for c in contacts {
                    // 起点不在“可交互”区域的触点，整段忽略（穿透给桌面）
                    match c.phase {
                        touch_evdev::Phase::Begin => {
                            if !point_input_ok(&app, c.x, c.y) {
                                ip.ignored_touch.insert(c.id);
                                continue;
                            }
                            ip.ignored_touch.remove(&c.id);
                        }
                        touch_evdev::Phase::Update => {
                            if ip.ignored_touch.contains(&c.id) {
                                continue;
                            }
                        }
                        touch_evdev::Phase::End => {
                            if ip.ignored_touch.remove(&c.id) {
                                continue;
                            }
                        }
                    }
                    let kind = match c.phase {
                        touch_evdev::Phase::Begin => TouchKind::Begin,
                        touch_evdev::Phase::Update => TouchKind::Update,
                        touch_evdev::Phase::End => TouchKind::End,
                    };
                    handle_touch(
                        &rt,
                        &mut app,
                        &mut ip,
                        wps_bridge.as_ref(),
                        c.id,
                        c.x,
                        c.y,
                        kind,
                        c.major,
                    );
                }
            }
        }
        // 1. X 事件
        loop {
            match x11.conn.poll_for_event() {
                Ok(Some(ev)) => {
                    let geometry_changed = match &ev {
                        Event::ConfigureNotify(e) => e.window == x11.root,
                        Event::PropertyNotify(e) => e.window == x11.root && e.atom == u32::from(x11rb::protocol::xproto::AtomEnum::RESOURCE_MANAGER),
                        _ => false,
                    };
                    if geometry_changed {
                        if let Some((w,h,scale)) = xp.sync_geometry()? {
                            cancel_input(&rt,&mut app,&mut ip);
                            active_x11_touches.clear();
                            app.configure_geometry(w,h,scale);
                            apply_input_shape(&rt,&app);
                            rt.redraw_full(&app);
                        }
                    }
                    match &ev {
                        Event::XinputTouchBegin(e) => {
                            active_x11_touches.insert((e.deviceid, e.detail));
                        }
                        Event::XinputTouchEnd(e) => {
                            active_x11_touches.remove(&(e.deviceid, e.detail));
                        }
                        _ => {}
                    }
                    handle_event(&rt, &mut app, &mut ip, wps_bridge.as_ref(), ev);
                }
                Ok(None) => break,
                Err(e) => {
                    return Err(format!("X11 桌面连接已断开：{e}").into());
                }
            }
        }

        // 2. 单实例唤醒
        if let Some(l) = single.as_ref() {
            if let Ok((mut c, _)) = l.accept() {
                let mut buf = [0u8; 16];
                let _ = c.set_read_timeout(Some(Duration::from_millis(20)));
                let n = c.read(&mut buf).unwrap_or(0);
                if &buf[..n] == b"toggle" {
                    if app.mode == 0 { switch_to_draw(&rt, &mut app, 1); }
                    else { switch_to_cursor(&rt, &mut app); }
                }
                let _ = x11.conn.map_window(win);
                let _ = xp.raise_overlays();
                let _ = x11.conn.flush();
                rt.redraw_full(&app);
            }
        }

        tick(&rt, &mut app, &mut ip, &mut wps_bridge, &mut timers);

        // 输入区域变化后刷新 Mutter 的指针命中缓存。等释放事件处理完，
        // 再重映射同一覆盖层；批注画布保留并在本帧重新上屏。
        let input_idle = active_x11_touches.is_empty() && !ip.dragging && ip.slider.is_none()
            && !app.is_drawing && !app.page_picker.as_ref().is_some_and(|p| p.dragging());
        if xp.refresh_input_window(input_idle)
            .map_err(|e| format!("刷新 X11 点击区域失败：{e}"))? {
            rt.redraw_full(&app);
        }

        // 一帧结束：合并相交脏区，保留左右工具栏独立刷新。
        rt.end_batch(&app);

        // Keep the low-latency frame rate while drawing or animating, but let
        // low-end classroom CPUs idle between frames. This also reduces X11
        // traffic while WPS is presenting and Sidera has no input.
        let busy = app.is_drawing || ip.dragging || ip.slider.is_some()
            || ip.pending_ui.is_some() || app.press.is_some()
            || app.launch.active() || ui::sidebar_animating(&app)
            || app.open_anim != 0 || app.page_picker.is_some();
        std::thread::sleep(Duration::from_millis(if busy { 4 } else { 12 }));
    }
}
