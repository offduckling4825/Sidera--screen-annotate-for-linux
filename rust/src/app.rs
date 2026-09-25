// ============================================================
// Sidera - 全局状态 / 常量 / 配置持久化
// 纯 Rust 重写，不依赖 Qt；X11 由 x11.rs 提供
// Copyright (C) 2026 Carl_Jin   GNU GPL v3
// ============================================================
use std::collections::{BTreeMap, HashMap, VecDeque};

use tiny_skia::{Color, Pixmap};

// ---------------- 常量（与原 C++ app.h 对齐） ----------------
pub const VERSION: &str = "Electro-rust-testing";
pub const MAX_UNDO: usize = 12;
pub const CACHE_WPS: i32 = 20;
pub const CACHE_BOARD: i32 = 10;
pub const CACHE_OTHER: i32 = 2;
pub const PALM_ERASE_WIDTH: i32 = 64;
pub const STROKE_RAMP_MS: i64 = 600;
pub const STROKE_MIN_SCALE: f64 = 0.85;

pub const WHITEBOARD_COLORS: [(u8, u8, u8); 2] = [(0x0F, 0x3D, 0x2E), (255, 255, 255)];

pub const COLORS: [(u8, u8, u8); 10] = [
    (255, 40, 40),
    (50, 120, 255),
    (40, 200, 60),
    (255, 210, 30),
    (240, 240, 240),
    (0x0A, 0xBA, 0xB5),
    (0xAB, 0x47, 0xBC),
    (0xC9, 0xDD, 0x22),
    (0xFF, 0x6D, 0x00),
    (0x79, 0x55, 0x48),
];
pub const PEN_SIZES: [i32; 3] = [3, 5, 10];
pub const ERASER_SIZES: [i32; 3] = [12, 24, 48];

pub fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_millis() as i64,
        Err(_) => 0,
    }
}

pub fn color(r: u8, g: u8, b: u8, a: u8) -> Color {
    Color::from_rgba8(r, g, b, a)
}

pub fn clone_pixmap(p: &Pixmap) -> Pixmap {
    let mut n = Pixmap::new(p.width(), p.height()).expect("pixmap");
    n.data_mut().copy_from_slice(p.data());
    n
}

// ---------------- 全局状态 ----------------
pub struct App {
    // 模式 0=光标 1=画笔 2=橡皮
    pub mode: i32,
    pub pen_popup_visible: bool,
    pub eraser_popup_visible: bool,
    pub more_menu_open: bool,
    pub settings_open: bool,
    pub popup_on_right: bool,

    // 白板
    pub whiteboard: bool,
    pub whiteboard_bg_index: i32,

    // 侧边栏
    pub collapsed: bool,
    pub sb_y: i32,
    pub collapse_ts: i64,

    // 颜色/粗细
    pub cur_color: usize,
    pub cur_pen: usize,
    pub cur_eraser: usize,

    // 绘制状态
    pub is_drawing: bool,
    pub last_pt: (i32, i32),
    pub stroke_start_ms: i64,
    pub last_pen_w: i32,

    // 侧边栏外观
    pub sb_scale: f64,
    pub sidebar_alpha: u8,

    // 平台
    pub hotkey_ok: bool,

    // WPS
    pub wps_fullscreen: bool,
    pub wps_debug: bool,
    pub wps_connected: bool,
    pub wps_real_pos: i32,

    // 多页缓存
    pub slide_cache: BTreeMap<i32, Pixmap>,
    pub whiteboard_cache: BTreeMap<i32, Pixmap>,
    pub current_slide: i32,
    pub saved_slide: i32,
    pub page_has_ink: bool,
    pub wb_limit_msg_until: i64,

    // 手掌/大触点橡皮
    pub palm_erase_preview: Vec<(i32, i32)>,
    pub palm_erase_w: i32,
    pub right_click_cursor_on: bool,
    pub erase_by_finger: bool,
    pub large_touch_threshold: f64,
    pub large_erase_scale10: i32,

    // 手势引擎
    pub t_prev_pos: HashMap<i32, (i32, i32)>,
    pub t_prev_role: HashMap<i32, i32>,
    pub t_prev_preview: Vec<(i32, i32)>,
    pub t_palm_latched: bool,
    pub t_moved: bool,
    pub t_begin_pos: (i32, i32),
    pub t_begin_role: i32,

    // 撤回栈（原始 RGBA 快照）
    pub undo: VecDeque<Vec<u8>>,

    // 画布
    pub canvas: Pixmap,
    pub screen_w: i32,
    pub screen_h: i32,

    // 撤销/调试提示：设置面板里“清空日志”反馈
    pub log_cleared_until: i64,
}

pub fn whiteboard_bg(sel: i32) -> Color {
    let i = sel.clamp(0, WHITEBOARD_COLORS.len() as i32 - 1) as usize;
    let (r, g, b) = WHITEBOARD_COLORS[i];
    color(r, g, b, 255)
}

impl App {
    pub fn new(canvas: Pixmap, screen_w: i32, screen_h: i32) -> Self {
        App {
            mode: 0,
            pen_popup_visible: false,
            eraser_popup_visible: false,
            more_menu_open: false,
            settings_open: false,
            popup_on_right: false,
            whiteboard: false,
            whiteboard_bg_index: 1,
            collapsed: false,
            sb_y: (screen_h - sb_height_of(1.0)) / 2,
            collapse_ts: 0,
            cur_color: 0,
            cur_pen: 1,
            cur_eraser: 1,
            is_drawing: false,
            last_pt: (0, 0),
            stroke_start_ms: 0,
            last_pen_w: 0,
            sb_scale: 1.0,
            sidebar_alpha: 80,
            hotkey_ok: false,
            wps_fullscreen: false,
            wps_debug: true,
            wps_connected: false,
            wps_real_pos: -1,
            slide_cache: BTreeMap::new(),
            whiteboard_cache: BTreeMap::new(),
            current_slide: 1,
            saved_slide: 1,
            page_has_ink: false,
            wb_limit_msg_until: 0,
            palm_erase_preview: Vec::new(),
            palm_erase_w: PALM_ERASE_WIDTH,
            right_click_cursor_on: true,
            erase_by_finger: true,
            large_touch_threshold: 64.0,
            large_erase_scale10: 12,
            t_prev_pos: HashMap::new(),
            t_prev_role: HashMap::new(),
            t_prev_preview: Vec::new(),
            t_palm_latched: false,
            t_moved: false,
            t_begin_pos: (0, 0),
            t_begin_role: 0,
            undo: VecDeque::new(),
            canvas,
            screen_w,
            screen_h,
            log_cleared_until: 0,
        }
    }

    // ---- 侧边栏尺寸 ----
    pub fn sb_width(&self) -> i32 {
        (56.0 * self.sb_scale) as i32
    }
    pub fn sb_btn(&self) -> i32 {
        (34.0 * self.sb_scale) as i32
    }
    pub fn sb_icon(&self) -> i32 {
        (24.0 * self.sb_scale) as i32
    }
    pub fn sb_dot(&self) -> i32 {
        (19.0 * self.sb_scale) as i32
    }
    pub fn sb_height(&self) -> i32 {
        // 22 + 11 个按钮 + 10 个间距(6)
        22 + 11 * self.sb_btn() + 10 * 6
    }
    pub fn collapsed_h(&self) -> i32 {
        self.sb_btn() * 3 + 12
    }
    pub fn sb_x_left(&self) -> i32 {
        4
    }
    pub fn sb_x_right(&self) -> i32 {
        self.screen_w - self.sb_width() - 4
    }

    pub fn pen_color(&self) -> Color {
        let (r, g, b) = COLORS[self.cur_color];
        color(r, g, b, 255)
    }
    pub fn pen_width(&self) -> i32 {
        PEN_SIZES[self.cur_pen]
    }
    pub fn eraser_width(&self) -> i32 {
        ERASER_SIZES[self.cur_eraser]
    }
    pub fn whiteboard_bg_color(&self) -> Color {
        whiteboard_bg(self.whiteboard_bg_index)
    }

    pub fn timed_pen_width(&self) -> i32 {
        let base = self.pen_width();
        if self.stroke_start_ms <= 0 {
            return base;
        }
        let minw = ((base as f64 * STROKE_MIN_SCALE).round() as i32).max(1);
        let dt = now_ms() - self.stroke_start_ms;
        let t = (dt as f64 / STROKE_RAMP_MS as f64).clamp(0.0, 1.0);
        minw + ((base - minw) as f64 * t).round() as i32
    }

    // ---- 撤回 ----
    pub fn clear_undo(&mut self) {
        self.undo.clear();
    }
    pub fn push_undo(&mut self) {
        if self.undo.len() >= MAX_UNDO {
            self.undo.pop_front();
        }
        self.undo.push_back(self.canvas.data().to_vec());
    }
    pub fn undo_last(&mut self) {
        if let Some(prev) = self.undo.pop_back() {
            self.canvas.data_mut().copy_from_slice(&prev);
        }
    }

    // ---- 手势复位 ----
    pub fn reset_palm_gesture(&mut self) {
        self.t_prev_pos.clear();
        self.t_prev_role.clear();
        self.t_prev_preview.clear();
        self.t_palm_latched = false;
        self.t_moved = false;
        self.t_begin_role = 0;
        self.palm_erase_preview.clear();
        self.palm_erase_w = PALM_ERASE_WIDTH;
    }

    // ---- 画布 ----
    pub fn clear_canvas(&mut self) {
        self.canvas.fill(Color::TRANSPARENT);
        self.page_has_ink = false;
        self.clear_undo();
    }

    // ---- 当前生效缓存 ----
    pub fn wps_mode_active(&self) -> bool {
        if self.whiteboard {
            return false;
        }
        self.wps_fullscreen || (self.wps_debug && self.wps_connected)
    }
    pub fn cache_capacity(&self) -> i32 {
        if self.whiteboard {
            CACHE_BOARD
        } else if self.wps_mode_active() {
            CACHE_WPS
        } else {
            CACHE_OTHER
        }
    }

    pub fn save_current_page(&mut self) {
        if !self.page_has_ink {
            return;
        }
        let cap = self.cache_capacity();
        let slide = self.current_slide;
        if self.whiteboard {
            if !self.whiteboard_cache.contains_key(&slide)
                && self.whiteboard_cache.len() as i32 >= cap
            {
                if let Some((&k, _)) = self.whiteboard_cache.iter().next() {
                    self.whiteboard_cache.remove(&k);
                }
            }
            let snap = clone_pixmap(&self.canvas);
            self.whiteboard_cache.insert(slide, snap);
        } else {
            if !self.slide_cache.contains_key(&slide)
                && self.slide_cache.len() as i32 >= cap
            {
                if self.wps_mode_active() {
                    return;
                }
                if let Some((&k, _)) = self.slide_cache.iter().next() {
                    self.slide_cache.remove(&k);
                }
            }
            let snap = clone_pixmap(&self.canvas);
            self.slide_cache.insert(slide, snap);
        }
    }

    pub fn load_page(&mut self, page: i32) {
        self.canvas.fill(Color::TRANSPARENT);
        let cached = if self.whiteboard {
            self.whiteboard_cache.get(&page)
        } else {
            self.slide_cache.get(&page)
        };
        if let Some(c) = cached {
            let mut snap = Pixmap::new(c.width(), c.height()).expect("pixmap");
            snap.data_mut().copy_from_slice(c.data());
            self.canvas = snap;
            self.page_has_ink = true;
        } else {
            self.page_has_ink = false;
        }
        self.clear_undo();
    }

    pub fn clear_all_pages(&mut self) {
        self.slide_cache.clear();
        self.whiteboard_cache.clear();
        self.current_slide = 1;
        self.page_has_ink = false;
        self.is_drawing = false;
        self.canvas.fill(Color::TRANSPARENT);
        self.clear_undo();
    }

    pub fn clear_current_strokes(&mut self) {
        if self.whiteboard {
            self.whiteboard_cache.remove(&self.current_slide);
        } else {
            self.slide_cache.remove(&self.current_slide);
        }
        self.canvas.fill(Color::TRANSPARENT);
        self.page_has_ink = false;
        self.clear_undo();
    }

    pub fn toggle_whiteboard(&mut self) {
        if !self.whiteboard {
            self.save_current_page();
            self.saved_slide = self.current_slide;
            self.whiteboard_cache.clear();
            self.whiteboard = true;
            self.current_slide = 1;
            self.canvas.fill(Color::TRANSPARENT);
            self.page_has_ink = false;
        } else {
            self.whiteboard_cache.clear();
            self.whiteboard = false;
            self.current_slide = self.saved_slide;
            self.canvas.fill(Color::TRANSPARENT);
            self.load_page(self.current_slide);
        }
        self.clear_undo();
    }
}

pub fn sb_height_of(scale: f64) -> i32 {
    22 + 11 * (34.0 * scale) as i32 + 60
}

// ---------------- 配置持久化 ~/.config/sidera/config ----------------
fn config_file() -> std::path::PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join(".config/sidera/config")
}

pub fn load_settings(app: &mut App) {
    let Ok(content) = std::fs::read_to_string(config_file()) else {
        return;
    };
    for line in content.lines() {
        let line = line.trim();
        let Some(eq) = line.find('=') else { continue };
        let k = &line[..eq];
        let v = &line[eq + 1..];
        match k {
            "wpsDebug" => app.wps_debug = v == "1",
            "sbScale" => {
                if let Ok(f) = v.parse::<f64>() {
                    app.sb_scale = f.clamp(0.6, 1.4);
                }
            }
            "sidebarAlpha" => {
                if let Ok(n) = v.parse::<i32>() {
                    app.sidebar_alpha = n.clamp(30, 255) as u8;
                }
            }
            "rightClickCursor" => app.right_click_cursor_on = v == "1",
            "eraseByFinger" => app.erase_by_finger = v == "1",
            "largeTouchThreshold" => {
                if let Ok(n) = v.parse::<i32>() {
                    app.large_touch_threshold = n.clamp(20, 260) as f64;
                }
            }
            "largeEraseScale10" => {
                if let Ok(n) = v.parse::<i32>() {
                    app.large_erase_scale10 = n.clamp(8, 25);
                }
            }
            _ => {}
        }
    }
}

pub fn save_settings(app: &App) {
    let path = config_file();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let content = format!(
        "wpsDebug={}\nsbScale={:.2}\nsidebarAlpha={}\nrightClickCursor={}\neraseByFinger={}\nlargeTouchThreshold={}\nlargeEraseScale10={}\n",
        if app.wps_debug { 1 } else { 0 },
        app.sb_scale,
        app.sidebar_alpha,
        if app.right_click_cursor_on { 1 } else { 0 },
        if app.erase_by_finger { 1 } else { 0 },
        app.large_touch_threshold as i32,
        app.large_erase_scale10,
    );
    let _ = std::fs::write(path, content);
}
