// ============================================================
// Sidera - 手绘 UI（侧边栏 / 弹窗 / 设置面板）与命中测试
// 不再使用 QWidget：所有控件都是几何矩形 + tiny-skia 绘制
// ============================================================
use tiny_skia::{
    BlendMode, Color, FillRule, LineCap, LineJoin, Paint, PathBuilder, Pixmap, Shader, Stroke,
    Transform,
};

use crate::app::{color, App};
use crate::text::Fonts;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct R {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl R {
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Btn {
    Collapse,
    Cursor,
    Pen,
    Eraser,
    Clear,
    Undo,
    Prev,
    Next,
    Exit,
    Whiteboard,
    More,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PopupHit {
    Color(usize),
    PenSize(usize),
    EraserSize(usize),
    ClearAll,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MoreHit {
    Screenshot,
    Settings,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SetHit {
    AlphaDec,
    AlphaInc,
    ScaleDec,
    ScaleInc,
    Autostart,
    WpsDebug,
    RightClick,
    EraseTrig,
    ThrDec,
    ThrInc,
    EraseScaleDec,
    EraseScaleInc,
    ClearLog,
    SysInfo,
    Quit,
    Done,
}

fn solid(c: Color) -> Paint<'static> {
    Paint {
        shader: Shader::SolidColor(c),
        anti_alias: true,
        blend_mode: BlendMode::SourceOver,
        ..Default::default()
    }
}

fn rounded_path(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<tiny_skia::Path> {
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.quad_to(x + w, y, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.quad_to(x + w, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.quad_to(x, y + h, x, y + h - r);
    pb.line_to(x, y + r);
    pb.quad_to(x, y, x + r, y);
    pb.close();
    pb.finish()
}

pub struct Ctx<'a> {
    pub pm: &'a mut Pixmap,
    pub tr: Transform,
}

impl<'a> Ctx<'a> {
    pub fn fill_round(&mut self, r: R, radius: f32, c: Color) {
        if let Some(p) = rounded_path(r.x as f32, r.y as f32, r.w as f32, r.h as f32, radius) {
            self.pm
                .fill_path(&p, &solid(c), FillRule::Winding, self.tr, None);
        }
    }
    pub fn stroke_round(&mut self, r: R, radius: f32, c: Color, width: f32) {
        if let Some(p) = rounded_path(r.x as f32, r.y as f32, r.w as f32, r.h as f32, radius) {
            let st = Stroke {
                width,
                line_cap: LineCap::Round,
                line_join: LineJoin::Round,
                ..Default::default()
            };
            self.pm
                .stroke_path(&p, &solid(c), &st, self.tr, None);
        }
    }
    pub fn fill_circle(&mut self, cx: f32, cy: f32, rad: f32, c: Color) {
        let mut pb = PathBuilder::new();
        pb.push_circle(cx, cy, rad);
        if let Some(p) = pb.finish() {
            self.pm
                .fill_path(&p, &solid(c), FillRule::Winding, self.tr, None);
        }
    }
    pub fn stroke_line(&mut self, a: (f32, f32), b: (f32, f32), c: Color, w: f32) {
        let st = Stroke {
            width: w,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            ..Default::default()
        };
        let mut pb = PathBuilder::new();
        pb.move_to(a.0, a.1);
        pb.line_to(b.0, b.1);
        if let Some(p) = pb.finish() {
            self.pm.stroke_path(&p, &solid(c), &st, self.tr, None);
        }
    }
    pub fn text(&mut self, fonts: &Fonts, s: &str, x: f32, baseline: f32, px: f32, c: Color) {
        fonts.draw(self.pm, s, x, baseline, px, c, self.tr);
    }
    pub fn text_center(&mut self, fonts: &Fonts, r: R, s: &str, px: f32, c: Color) {
        let w = fonts.text_width(s, px);
        let x = r.x as f32 + (r.w as f32 - w) / 2.0;
        let baseline = r.y as f32 + r.h as f32 / 2.0 + px * 0.35;
        self.text(fonts, s, x, baseline, px, c);
    }
    pub fn text_multiline_center(&mut self, fonts: &Fonts, r: R, s: &str, px: f32, c: Color) {
        let lines: Vec<&str> = s.split('\n').collect();
        let line_h = px * 1.15;
        let total = line_h * lines.len() as f32;
        let mut y = r.y as f32 + (r.h as f32 - total) / 2.0 + px * 0.9;
        for line in lines {
            let w = fonts.text_width(line, px);
            let x = r.x as f32 + (r.w as f32 - w) / 2.0;
            self.text(fonts, line, x, y, px, c);
            y += line_h;
        }
    }
}

// ---------------- 几何 ----------------
pub fn sidebar_rect(app: &App, right: bool) -> R {
    let x = if right { app.sb_x_right() } else { app.sb_x_left() };
    let h = if app.collapsed {
        app.collapsed_h()
    } else {
        app.sb_height()
    };
    R {
        x,
        y: app.sb_y,
        w: app.sb_width(),
        h,
    }
}

pub fn sidebar_buttons(app: &App, right: bool) -> Vec<(Btn, R)> {
    let sb = sidebar_rect(app, right);
    if app.collapsed {
        return vec![(
            Btn::Collapse,
            R {
                x: sb.x + 4,
                y: sb.y + 6,
                w: sb.w - 8,
                h: sb.h - 12,
            },
        )];
    }
    let btn = app.sb_btn();
    let bx = sb.x + (sb.w - btn) / 2;
    let mut y = sb.y + 12;
    let order = [
        Btn::Collapse,
        Btn::Cursor,
        Btn::Pen,
        Btn::Eraser,
        Btn::Clear,
        Btn::Undo,
        Btn::Prev,
        Btn::Next,
        Btn::Exit,
        Btn::Whiteboard,
        Btn::More,
    ];
    let mut v = Vec::new();
    for k in order {
        v.push((k, R { x: bx, y, w: btn, h: btn }));
        y += btn + 6;
    }
    v
}

pub fn pen_popup_rect(app: &App) -> R {
    let w = 200;
    let h = 224;
    let sb = sidebar_rect(app, app.popup_on_right);
    let x = if app.popup_on_right {
        sb.x - w - 8
    } else {
        sb.x + sb.w + 8
    };
    let y = sb.y + (sb.h - h) / 2;
    R {
        x: x.clamp(0, (app.screen_w - w).max(0)),
        y: y.clamp(0, (app.screen_h - h).max(0)),
        w,
        h,
    }
}

pub fn eraser_popup_rect(app: &App) -> R {
    let w = 220;
    let h = 150;
    let sb = sidebar_rect(app, app.popup_on_right);
    let x = if app.popup_on_right {
        sb.x - w - 8
    } else {
        sb.x + sb.w + 8
    };
    let y = sb.y + (sb.h - h) / 2;
    R {
        x: x.clamp(0, (app.screen_w - w).max(0)),
        y: y.clamp(0, (app.screen_h - h).max(0)),
        w,
        h,
    }
}

pub fn more_menu_rect(app: &App) -> R {
    let sb = sidebar_rect(app, app.popup_on_right);
    let w = 140;
    let h = 80;
    let x = if app.popup_on_right {
        sb.x - w - 8
    } else {
        sb.x + sb.w + 8
    };
    let more_y = sidebar_buttons(app, app.popup_on_right)
        .iter()
        .find(|(k, _)| *k == Btn::More)
        .map(|(_, r)| r.y)
        .unwrap_or(sb.y);
    let y = more_y.clamp(0, (app.screen_h - h).max(0));
    R {
        x: x.clamp(0, (app.screen_w - w).max(0)),
        y,
        w,
        h,
    }
}

pub fn pen_popup_hit(app: &App, x: i32, y: i32) -> Option<PopupHit> {
    let p = pen_popup_rect(app);
    if !p.contains(x, y) {
        return None;
    }
    let start_y = p.y + 34;
    for i in 0..10 {
        let row = (i / 4) as i32;
        let col = (i % 4) as i32;
        let cx = p.x + 12 + col * 38 + 15;
        let cy = start_y + row * 38 + 15;
        let rr = R {
            x: cx - 15,
            y: cy - 15,
            w: 30,
            h: 30,
        };
        if rr.contains(x, y) {
            return Some(PopupHit::Color(i));
        }
    }
    let sy = p.y + 172;
    let bw = (p.w - 24 - 16) / 3;
    for i in 0..3 {
        let bx = p.x + 12 + i as i32 * (bw + 8);
        let rr = R { x: bx, y: sy, w: bw, h: 36 };
        if rr.contains(x, y) {
            return Some(PopupHit::PenSize(i as usize));
        }
    }
    None
}

pub fn eraser_popup_hit(app: &App, x: i32, y: i32) -> Option<PopupHit> {
    let p = eraser_popup_rect(app);
    if !p.contains(x, y) {
        return None;
    }
    let sy = p.y + 34;
    let bw = (p.w - 24 - 16) / 3;
    for i in 0..3 {
        let bx = p.x + 12 + i as i32 * (bw + 8);
        let rr = R { x: bx, y: sy, w: bw, h: 36 };
        if rr.contains(x, y) {
            return Some(PopupHit::EraserSize(i as usize));
        }
    }
    let cy = p.y + 82;
    let rr = R { x: p.x + 12, y: cy, w: p.w - 24, h: 38 };
    if rr.contains(x, y) {
        return Some(PopupHit::ClearAll);
    }
    None
}

pub fn more_menu_hit(app: &App, x: i32, y: i32) -> Option<MoreHit> {
    let m = more_menu_rect(app);
    if !m.contains(x, y) {
        return None;
    }
    if y < m.y + 36 {
        Some(MoreHit::Screenshot)
    } else {
        Some(MoreHit::Settings)
    }
}

pub fn settings_rect(app: &App) -> R {
    let w = 380;
    let h = 640;
    R {
        x: (app.screen_w - w) / 2,
        y: ((app.screen_h - h) / 2).max(8),
        w,
        h,
    }
}

pub fn settings_controls(app: &App) -> Vec<(SetHit, R)> {
    let p = settings_rect(app);
    let mut y = p.y + 92;
    let step = 44;
    let full = |ry: i32| R {
        x: p.x + 14,
        y: ry,
        w: p.w - 28,
        h: 32,
    };
    let dec = |ry: i32| R { x: p.x + 160, y: ry, w: 32, h: 32 };
    let inc = |ry: i32| R { x: p.x + 264, y: ry, w: 32, h: 32 };
    let mut v = Vec::new();
    v.push((SetHit::AlphaDec, dec(y)));
    v.push((SetHit::AlphaInc, inc(y)));
    y += step;
    v.push((SetHit::ScaleDec, dec(y)));
    v.push((SetHit::ScaleInc, inc(y)));
    y += step;
    v.push((SetHit::Autostart, full(y)));
    y += step;
    v.push((SetHit::WpsDebug, full(y)));
    y += step;
    v.push((SetHit::RightClick, full(y)));
    y += step;
    v.push((SetHit::EraseTrig, full(y)));
    y += step;
    v.push((SetHit::ThrDec, dec(y)));
    v.push((SetHit::ThrInc, inc(y)));
    y += step;
    v.push((SetHit::EraseScaleDec, dec(y)));
    v.push((SetHit::EraseScaleInc, inc(y)));
    y += step;
    v.push((SetHit::ClearLog, full(y)));
    y += step;
    v.push((SetHit::SysInfo, full(y)));
    y += step;
    v.push((SetHit::Quit, full(y)));
    y += step;
    v.push((SetHit::Done, full(y)));
    v
}

pub fn settings_hit(app: &App, x: i32, y: i32) -> Option<SetHit> {
    if !settings_rect(app).contains(x, y) {
        return None;
    }
    for (k, r) in settings_controls(app) {
        if r.contains(x, y) {
            return Some(k);
        }
    }
    None
}

// ---------------- 设置面板用到的外部状态 ----------------
pub fn autostart_path() -> std::path::PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join("autostart/sidera.desktop")
}

pub fn is_autostart() -> bool {
    autostart_path().exists()
}

pub fn set_autostart(on: bool) {
    let p = autostart_path();
    if on {
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(
            p,
            "[Desktop Entry]\nType=Application\nName=Sidera\nComment=Sidera软件\nExec=sidera\nTerminal=false\n",
        );
    } else {
        let _ = std::fs::remove_file(p);
    }
}

// ---------------- 绘制 ----------------
pub fn render_region(
    app: &App,
    fonts: &Fonts,
    icon: Option<&Pixmap>,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) -> Option<Pixmap> {
    let x = x.max(0);
    let y = y.max(0);
    let x2 = (x + w).min(app.screen_w);
    let y2 = (y + h).min(app.screen_h);
    let rw = (x2 - x) as u32;
    let rh = (y2 - y) as u32;
    if rw == 0 || rh == 0 {
        return None;
    }
    let mut pm = Pixmap::new(rw, rh)?;
    let tr = Transform::from_translate(-x as f32, -y as f32);

    // 背景
    if app.whiteboard {
        pm.fill(app.whiteboard_bg_color());
    }

    // 画布
    pm.draw_pixmap(
        -x,
        -y,
        app.canvas.as_ref(),
        &tiny_skia::PixmapPaint::default(),
        Transform::identity(),
        None,
    );

    {
        let mut ctx = Ctx {
            pm: &mut pm,
            tr,
        };

        // 手掌/大触点橡皮圆形预览
        if !app.palm_erase_preview.is_empty() {
            let ew = if app.mode == 2 {
                app.eraser_width()
            } else {
                app.palm_erase_w
            } as f32;
            for &(cx, cy) in &app.palm_erase_preview {
                ctx.fill_circle(cx as f32, cy as f32, ew / 2.0, color(255, 150, 190, 55));
                let half = (ew / 2.0) as i32;
                ctx.stroke_round(
                    R {
                        x: cx - half,
                        y: cy - half,
                        w: half * 2,
                        h: half * 2,
                    },
                    ew / 2.0,
                    color(255, 255, 255, 190),
                    2.0,
                );
            }
        }

        // 白板页码
        if app.whiteboard {
            let s = format!("第 {} 页 / {}", app.current_slide, crate::app::CACHE_BOARD);
            ctx.text(
                fonts,
                &s,
                16.0,
                app.screen_h as f32 - 22.0,
                20.0,
                color(0x0A, 0xBA, 0xB5, 255),
            );
            if crate::app::now_ms() < app.wb_limit_msg_until {
                let r = R {
                    x: 0,
                    y: 0,
                    w: app.screen_w,
                    h: app.screen_h,
                };
                ctx.text_center(
                    fonts,
                    r,
                    "已达到白板页数上限",
                    34.0,
                    color(0x0A, 0xBA, 0xB5, 255),
                );
            }
        }

        draw_sidebar(app, fonts, &mut ctx, false);
        draw_sidebar(app, fonts, &mut ctx, true);

        if app.pen_popup_visible {
            draw_pen_popup(app, fonts, &mut ctx);
        }
        if app.eraser_popup_visible {
            draw_eraser_popup(app, fonts, &mut ctx);
        }
        if app.more_menu_open {
            draw_more_menu(fonts, &mut ctx, app);
        }
        if app.settings_open {
            draw_settings(app, fonts, &mut ctx, icon);
        }
    }

    Some(pm)
}

fn draw_sidebar(app: &App, fonts: &Fonts, ctx: &mut Ctx, right: bool) {
    let sb = sidebar_rect(app, right);
    let rad = sb.w as f32 / 2.0;
    ctx.fill_round(sb, rad, color(42, 42, 50, app.sidebar_alpha));
    ctx.stroke_round(sb, rad, color(90, 90, 100, 255), 2.0);

    if !app.collapsed {
        let d = app.sb_dot() as f32;
        let cx = sb.x as f32 + sb.w as f32 / 2.0;
        let cy = sb.y as f32 + d / 4.0;
        ctx.fill_circle(cx, cy, d / 2.0, color(80, 85, 100, 240));
        ctx.fill_circle(cx, cy, d / 2.0 - 1.5, color(80, 85, 100, 240));
    }

    for (k, r) in sidebar_buttons(app, right) {
        draw_btn(app, fonts, ctx, k, r);
    }
}

fn draw_btn(app: &App, fonts: &Fonts, ctx: &mut Ctx, k: Btn, r: R) {
    let btn = app.sb_btn() as f32;
    match k {
        Btn::Collapse => {
            ctx.fill_round(r, btn / 2.0, color(0x22, 0x24, 0x36, 255));
            ctx.stroke_round(r, btn / 2.0, color(255, 213, 74, 153), 2.0);
            let t = if app.collapsed {
                "展\n开\n侧\n边\n栏"
            } else {
                "收缩"
            };
            ctx.text_multiline_center(fonts, r, t, (btn * 13.0 / 38.0).max(9.0), color(255, 213, 74, 255));
        }
        Btn::Cursor | Btn::Pen | Btn::Eraser => {
            let idx = match k {
                Btn::Cursor => 0,
                Btn::Pen => 1,
                _ => 2,
            };
            if app.mode == idx {
                ctx.fill_round(r, btn / 2.0, color(10, 186, 181, 102));
                ctx.stroke_round(r, btn / 2.0, color(0x7f, 0xe9, 0xe4, 255), 2.0);
            }
            let s = app.sb_icon() as f32;
            let ox = r.x as f32 + (r.w as f32 - s) / 2.0;
            let oy = r.y as f32 + (r.h as f32 - s) / 2.0;
            match k {
                Btn::Cursor => icon_cursor(ctx, ox, oy, s),
                Btn::Pen => icon_pen(ctx, ox, oy, s),
                _ => icon_eraser(ctx, ox, oy, s),
            }
        }
        Btn::Clear => {
            ctx.fill_round(r, btn * 0.35, color(0x00, 0xbc, 0xd4, 255));
            ctx.stroke_round(r, btn * 0.35, color(0x4d, 0xd0, 0xe1, 255), 2.0);
            ctx.text_center(fonts, r, "清除", (btn * 16.0 / 38.0).max(10.0), color(0x06, 0x34, 0x3a, 255));
        }
        Btn::Undo => {
            ctx.stroke_round(r, btn * 0.35, color(255, 154, 60, 179), 2.0);
            ctx.text_center(fonts, r, "撤回", (btn * 16.0 / 38.0).max(10.0), color(255, 154, 60, 255));
        }
        Btn::Prev | Btn::Next => {
            ctx.fill_round(r, btn / 2.0, color(0x3a, 0x3a, 0x4a, 255));
            ctx.stroke_round(r, btn / 2.0, color(0x66, 0x88, 0xcc, 255), 1.5);
            let t = if k == Btn::Prev { "▲" } else { "▼" };
            ctx.text_center(fonts, r, t, (btn * 12.0 / 19.0).max(10.0), color(0xcc, 0xcc, 0xff, 255));
        }
        Btn::Exit => {
            if app.whiteboard {
                let c = app.whiteboard_bg_color();
                ctx.fill_round(r, 4.0, c);
                ctx.stroke_round(r, 4.0, Color::WHITE, 2.0);
                let idx = app.whiteboard_bg_index.clamp(0, 1) as usize;
                let (rr, gg, bb) = crate::app::WHITEBOARD_COLORS[idx];
                let light = 0.299 * rr as f32 + 0.587 * gg as f32 + 0.114 * bb as f32;
                let fg = if light > 128.0 {
                    color(0x11, 0x11, 0x11, 255)
                } else {
                    Color::WHITE
                };
                ctx.text_multiline_center(fonts, r, "背景\n颜色", (btn * 11.0 / 34.0).max(9.0), fg);
            } else {
                ctx.fill_round(r, 4.0, color(0xd3, 0x3a, 0x3a, 255));
                ctx.stroke_round(r, 4.0, color(0xff, 0x80, 0x80, 255), 2.0);
                ctx.text_multiline_center(fonts, r, "退出\n放映", (btn * 12.0 / 34.0).max(9.0), Color::WHITE);
            }
        }
        Btn::Whiteboard => {
            let (fill, txt, border) = if app.whiteboard {
                (color(0xf4, 0xf4, 0xf4, 255), color(0x11, 0x11, 0x11, 255), color(0xff, 0x98, 0x00, 255))
            } else {
                (color(0x55, 0x55, 0x55, 255), color(0xee, 0xee, 0xee, 255), color(0x99, 0x99, 0x99, 255))
            };
            ctx.fill_round(r, btn * 0.35, fill);
            ctx.stroke_round(r, btn * 0.35, border, 2.0);
            ctx.text_center(fonts, r, "白板", (btn * 16.0 / 38.0).max(10.0), txt);
        }
        Btn::More => {
            ctx.fill_round(r, btn / 2.0, color(0, 0, 0, 255));
            ctx.stroke_round(r, btn / 2.0, color(0x33, 0x33, 0x33, 255), 2.0);
            ctx.text_center(fonts, r, "⋯", (btn * 16.0 / 38.0).max(12.0), Color::WHITE);
        }
    }
}

fn icon_cursor(ctx: &mut Ctx, ox: f32, oy: f32, s: f32) {
    let mut pb = PathBuilder::new();
    pb.move_to(ox + s * 0.28, oy + s * 0.14);
    pb.line_to(ox + s - s * 0.14, oy + s * 0.55);
    pb.line_to(ox + s * 0.55, oy + s * 0.55);
    pb.line_to(ox + s * 0.55, oy + s - s * 0.14);
    pb.close();
    if let Some(p) = pb.finish() {
        ctx.pm
            .fill_path(&p, &solid(color(255, 255, 255, 200)), FillRule::Winding, ctx.tr, None);
        let st = Stroke {
            width: 2.5,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            ..Default::default()
        };
        ctx.pm.stroke_path(&p, &solid(Color::WHITE), &st, ctx.tr, None);
    }
}

fn icon_pen(ctx: &mut Ctx, ox: f32, oy: f32, s: f32) {
    let m = s * 0.15;
    ctx.stroke_line(
        (ox + s - m, oy + m),
        (ox + m + 2.0, oy + s - m - 2.0),
        color(255, 200, 100, 255),
        3.5,
    );
    ctx.stroke_line(
        (ox + m + 2.0, oy + s - m - 2.0),
        (ox + m * 0.3, oy + s - m * 0.3),
        color(40, 40, 40, 255),
        4.5,
    );
}

fn icon_eraser(ctx: &mut Ctx, ox: f32, oy: f32, s: f32) {
    let m = s * 0.18;
    let r = R {
        x: (ox + m * 1.5) as i32,
        y: (oy + m) as i32,
        w: (s - m * 3.0).max(1.0) as i32,
        h: (s - m * 2.0).max(1.0) as i32,
    };
    ctx.fill_round(r, m * 0.8, color(255, 150, 150, 200));
    ctx.stroke_round(r, m * 0.8, color(255, 180, 180, 255), 2.5);
}

fn draw_pen_popup(app: &App, fonts: &Fonts, ctx: &mut Ctx) {
    let p = pen_popup_rect(app);
    ctx.fill_round(p, 14.0, color(42, 42, 50, 245));
    ctx.stroke_round(p, 14.0, color(0x66, 0x66, 0x66, 255), 2.0);
    ctx.text(fonts, "画笔颜色", (p.x + 12) as f32, (p.y + 26) as f32, 15.0, color(0xcc, 0xcc, 0xcc, 255));
    let start_y = p.y + 34;
    for i in 0..10usize {
        let row = (i / 4) as i32;
        let col = (i % 4) as i32;
        let (r, g, b) = crate::app::COLORS[i];
        let cx = (p.x + 12 + col * 38 + 15) as f32;
        let cy = (start_y + row * 38 + 15) as f32;
        ctx.fill_circle(cx, cy, 15.0, color(r, g, b, 255));
        let bd = if i == app.cur_color {
            Color::WHITE
        } else {
            color(0x66, 0x66, 0x66, 255)
        };
        let w = if i == app.cur_color { 3.0 } else { 2.0 };
        ctx.stroke_round(
            R {
                x: cx as i32 - 15,
                y: cy as i32 - 15,
                w: 30,
                h: 30,
            },
            15.0,
            bd,
            w,
        );
    }
    let sy = p.y + 172;
    ctx.text(fonts, "画笔粗细", (p.x + 12) as f32, (sy - 10) as f32, 15.0, color(0xcc, 0xcc, 0xcc, 255));
    let bw = (p.w - 24 - 16) / 3;
    let labels = ["细 3", "中 6", "粗 10"];
    for i in 0..3usize {
        let bx = p.x + 12 + i as i32 * (bw + 8);
        let r = R { x: bx, y: sy, w: bw, h: 36 };
        let sel = i == app.cur_pen;
        ctx.fill_round(
            r,
            6.0,
            if sel {
                color(0x33, 0x77, 0xcc, 255)
            } else {
                color(0x44, 0x44, 0x44, 255)
            },
        );
        ctx.stroke_round(r, 6.0, if sel { color(0x55, 0x99, 0xff, 255) } else { color(0x66, 0x66, 0x66, 255) }, 2.0);
        ctx.text_center(fonts, r, labels[i], 14.0, Color::WHITE);
    }
}

fn draw_eraser_popup(app: &App, fonts: &Fonts, ctx: &mut Ctx) {
    let p = eraser_popup_rect(app);
    ctx.fill_round(p, 14.0, color(42, 42, 50, 245));
    ctx.stroke_round(p, 14.0, color(0x66, 0x66, 0x66, 255), 2.0);
    ctx.text(fonts, "橡皮擦大小", (p.x + 12) as f32, (p.y + 26) as f32, 15.0, color(0xcc, 0xcc, 0xcc, 255));
    let sy = p.y + 34;
    let bw = (p.w - 24 - 16) / 3;
    let labels = ["小 12", "中 24", "大 48"];
    for i in 0..3usize {
        let bx = p.x + 12 + i as i32 * (bw + 8);
        let r = R { x: bx, y: sy, w: bw, h: 36 };
        let sel = i == app.cur_eraser;
        ctx.fill_round(
            r,
            6.0,
            if sel {
                color(0xff, 0x88, 0x00, 255)
            } else {
                color(0x44, 0x44, 0x44, 255)
            },
        );
        ctx.stroke_round(r, 6.0, if sel { color(0xff, 0xaa, 0x00, 255) } else { color(0x66, 0x66, 0x66, 255) }, 2.0);
        let fg = if sel { color(0, 0, 0, 255) } else { Color::WHITE };
        ctx.text_center(fonts, r, labels[i], 14.0, fg);
    }
    let cy = p.y + 82;
    let r = R { x: p.x + 12, y: cy, w: p.w - 24, h: 38 };
    ctx.fill_round(r, 6.0, color(0x55, 0x55, 0x55, 255));
    ctx.text_center(fonts, r, "清除全部", 14.0, Color::WHITE);
}

fn draw_more_menu(fonts: &Fonts, ctx: &mut Ctx, app: &App) {
    let m = more_menu_rect(app);
    ctx.fill_round(m, 8.0, color(43, 43, 51, 250));
    ctx.stroke_round(m, 8.0, color(0x55, 0x55, 0x55, 255), 1.5);
    let r1 = R { x: m.x, y: m.y, w: m.w, h: 36 };
    let r2 = R { x: m.x, y: m.y + 36, w: m.w, h: 36 };
    ctx.text_center(fonts, r1, "截图", 14.0, color(0xee, 0xee, 0xee, 255));
    ctx.text_center(fonts, r2, "设置", 14.0, color(0xee, 0xee, 0xee, 255));
}

fn draw_settings(app: &App, fonts: &Fonts, ctx: &mut Ctx, icon: Option<&Pixmap>) {
    let p = settings_rect(app);
    ctx.fill_round(p, 10.0, color(0x2b, 0x2b, 0x33, 250));
    ctx.stroke_round(p, 10.0, color(0x55, 0x55, 0x55, 255), 2.0);

    // 顶部图标 + 标题
    if let Some(ico) = icon {
        let target = 44;
        let ix = p.x + (p.w - target) / 2;
        ctx.pm.draw_pixmap(
            ix,
            p.y + 8,
            ico.as_ref(),
            &tiny_skia::PixmapPaint::default(),
            ctx.tr,
            None,
        );
    }
    ctx.text_center(
        fonts,
        R {
            x: p.x,
            y: p.y + 56,
            w: p.w,
            h: 24,
        },
        "Sidera 设置",
        16.0,
        color(0xee, 0xee, 0xee, 255),
    );

    let controls = settings_controls(app);
    let label = |hit: SetHit| controls.iter().find(|(k, _)| *k == hit).map(|(_, r)| *r);
    let row_y = |hit: SetHit| label(hit).map(|r| r.y).unwrap_or(0);

    let label_color = color(0xcc, 0xcc, 0xcc, 255);
    let val_color = color(0xaa, 0xdd, 0xff, 255);
    let toggle_col = color(0x44, 0x44, 0x44, 255);

    // 1 alpha
    {
        let ry = row_y(SetHit::AlphaDec);
        ctx.text(fonts, "侧边栏透明度", (p.x + 14) as f32, (ry + 22) as f32, 14.0, label_color);
        if let Some(r) = label(SetHit::AlphaDec) {
            ctx.fill_round(r, 6.0, toggle_col);
            ctx.text_center(fonts, r, "−", 18.0, Color::WHITE);
        }
        if let Some(r) = label(SetHit::AlphaInc) {
            ctx.fill_round(r, 6.0, toggle_col);
            ctx.text_center(fonts, r, "+", 18.0, Color::WHITE);
        }
        ctx.text_center(
            fonts,
            R { x: p.x + 196, y: ry, w: 64, h: 32 },
            &format!("{}", app.sidebar_alpha),
            14.0,
            val_color,
        );
    }
    // 2 scale
    {
        let ry = row_y(SetHit::ScaleDec);
        ctx.text(fonts, "侧边栏大小", (p.x + 14) as f32, (ry + 22) as f32, 14.0, label_color);
        if let Some(r) = label(SetHit::ScaleDec) {
            ctx.fill_round(r, 6.0, toggle_col);
            ctx.text_center(fonts, r, "−", 18.0, Color::WHITE);
        }
        if let Some(r) = label(SetHit::ScaleInc) {
            ctx.fill_round(r, 6.0, toggle_col);
            ctx.text_center(fonts, r, "+", 18.0, Color::WHITE);
        }
        ctx.text_center(
            fonts,
            R { x: p.x + 196, y: ry, w: 64, h: 32 },
            &format!("{:.1}", app.sb_scale),
            14.0,
            val_color,
        );
    }
    // 切换行
    let toggle = |ctx: &mut Ctx, hit: SetHit, text: &str, on: bool, on_col: Color| {
        if let Some(r) = label(hit) {
            ctx.fill_round(r, 6.0, if on { on_col } else { toggle_col });
            ctx.text_center(fonts, r, text, 14.0, Color::WHITE);
        }
    };
    toggle(
        ctx,
        SetHit::Autostart,
        &format!("开机自启动: {}", if is_autostart() { "开" } else { "关" }),
        is_autostart(),
        color(0x2a, 0x6e, 0x3f, 255),
    );
    toggle(
        ctx,
        SetHit::WpsDebug,
        &format!("WPS 接口调试: {}", if app.wps_debug { "开" } else { "关" }),
        app.wps_debug,
        color(0x6a, 0x3f, 0x2a, 255),
    );
    toggle(
        ctx,
        SetHit::RightClick,
        &format!("右键归位光标: {}", if app.right_click_cursor_on { "开" } else { "关" }),
        app.right_click_cursor_on,
        color(0x2a, 0x5a, 0x6a, 255),
    );
    toggle(
        ctx,
        SetHit::EraseTrig,
        &format!(
            "橡皮触发: {}",
            if app.erase_by_finger { "多指" } else { "手背" }
        ),
        true,
        color(0x2a, 0x5a, 0x6a, 255),
    );
    // 3 threshold
    {
        let ry = row_y(SetHit::ThrDec);
        ctx.text(fonts, "大触点阈值 (px)", (p.x + 14) as f32, (ry + 22) as f32, 14.0, label_color);
        if let Some(r) = label(SetHit::ThrDec) {
            ctx.fill_round(r, 6.0, toggle_col);
            ctx.text_center(fonts, r, "−", 18.0, Color::WHITE);
        }
        if let Some(r) = label(SetHit::ThrInc) {
            ctx.fill_round(r, 6.0, toggle_col);
            ctx.text_center(fonts, r, "+", 18.0, Color::WHITE);
        }
        ctx.text_center(
            fonts,
            R { x: p.x + 196, y: ry, w: 64, h: 32 },
            &format!("{}", app.large_touch_threshold as i32),
            14.0,
            val_color,
        );
    }
    // 4 erasescale
    {
        let ry = row_y(SetHit::EraseScaleDec);
        ctx.text(fonts, "大触点橡皮倍率", (p.x + 14) as f32, (ry + 22) as f32, 14.0, label_color);
        if let Some(r) = label(SetHit::EraseScaleDec) {
            ctx.fill_round(r, 6.0, toggle_col);
            ctx.text_center(fonts, r, "−", 18.0, Color::WHITE);
        }
        if let Some(r) = label(SetHit::EraseScaleInc) {
            ctx.fill_round(r, 6.0, toggle_col);
            ctx.text_center(fonts, r, "+", 18.0, Color::WHITE);
        }
        ctx.text_center(
            fonts,
            R { x: p.x + 196, y: ry, w: 64, h: 32 },
            &format!("{:.1}", app.large_erase_scale10 as f32 / 10.0),
            14.0,
            val_color,
        );
    }
    // 动作行
    let log_txt = if crate::app::now_ms() < app.log_cleared_until {
        "已清空 ✓"
    } else {
        "清空调试日志"
    };
    if let Some(r) = label(SetHit::ClearLog) {
        ctx.fill_round(r, 6.0, toggle_col);
        ctx.text_center(fonts, r, log_txt, 14.0, color(0xcc, 0xcc, 0xcc, 255));
    }
    if let Some(r) = label(SetHit::SysInfo) {
        ctx.fill_round(r, 6.0, toggle_col);
        ctx.text_center(fonts, r, "系统诊断信息", 14.0, color(0xcc, 0xcc, 0xcc, 255));
    }
    if let Some(r) = label(SetHit::Quit) {
        ctx.fill_round(r, 6.0, color(0x8a, 0x2f, 0x2f, 255));
        ctx.text_center(fonts, r, "退出软件", 14.0, Color::WHITE);
    }
    if let Some(r) = label(SetHit::Done) {
        ctx.fill_round(r, 6.0, color(0x33, 0x77, 0xcc, 255));
        ctx.text_center(fonts, r, "完成", 15.0, Color::WHITE);
    }

    // 版权
    ctx.text_center(
        fonts,
        R {
            x: p.x,
            y: p.y + p.h - 26,
            w: p.w,
            h: 20,
        },
        "Sidera Electro-rust-testing  © 2026 Carl_Jin  GNU GPL v3",
        11.0,
        color(0x55, 0x55, 0x66, 255),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        let mut pm = Pixmap::new(2560, 1600).unwrap();
        pm.fill(Color::TRANSPARENT);
        App::new(pm, 2560, 1600)
    }

    #[test]
    fn settings_controls_are_hittable() {
        let app = app();
        for (hit, r) in settings_controls(&app) {
            let got = settings_hit(&app, r.x + r.w / 2, r.y + r.h / 2);
            assert_eq!(got, Some(hit), "控件 {:?} 矩形 {:?} 命中失败", hit, r);
        }
    }

    #[test]
    fn sidebar_buttons_are_hittable() {
        let app = app();
        for right in [false, true] {
            for (k, r) in sidebar_buttons(&app, right) {
                assert!(r.contains(r.x + r.w / 2, r.y + r.h / 2), "{:?} 不在自身矩形内", k);
            }
        }
    }

    #[test]
    fn more_menu_items_hittable() {
        let mut app = app();
        app.more_menu_open = true;
        let m = more_menu_rect(&app);
        assert_eq!(
            more_menu_hit(&app, m.x + m.w / 2, m.y + 18),
            Some(MoreHit::Screenshot)
        );
        assert_eq!(
            more_menu_hit(&app, m.x + m.w / 2, m.y + 54),
            Some(MoreHit::Settings)
        );
    }
}
