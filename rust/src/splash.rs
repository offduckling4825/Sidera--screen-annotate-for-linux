// ============================================================
// Sidera - 启动闪屏（青→粉渐变 + 进度条）
// ============================================================
use std::time::{Duration, Instant};

use tiny_skia::{
    Color, FillRule, GradientStop, LinearGradient, Paint, PathBuilder, Pixmap, Point, Shader,
    SpreadMode, Transform,
};

use crate::text::Fonts;
use crate::x11::X11;

fn rounded(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<tiny_skia::Path> {
    let r = r.min(w / 2.0).min(h / 2.0);
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

fn paint_shader(shader: Shader<'static>) -> Paint<'static> {
    Paint {
        shader,
        anti_alias: true,
        ..Default::default()
    }
}

fn draw(fonts: &Fonts, icon: Option<&Pixmap>, w: u32, h: u32, progress: u32) -> Pixmap {
    let mut pm = Pixmap::new(w, h).unwrap();
    let fw = w as f32;
    let fh = h as f32;
    // 背景渐变
    if let Some(sh) = LinearGradient::new(
        Point::from_xy(0.0, 0.0),
        Point::from_xy(fw, fh),
        vec![
            GradientStop::new(0.0, Color::from_rgba8(0, 200, 255, 245)),
            GradientStop::new(1.0, Color::from_rgba8(255, 120, 180, 245)),
        ],
        SpreadMode::Pad,
        Transform::identity(),
    ) {
        if let Some(p) = rounded(1.5, 1.5, fw - 3.0, fh - 3.0, 20.0) {
            pm.fill_path(&p, &paint_shader(sh), FillRule::Winding, Transform::identity(), None);
        }
    }
    // 图标
    if let Some(ico) = icon {
        pm.draw_pixmap(
            ((w - ico.width()) / 2) as i32,
            22,
            ico.as_ref(),
            &tiny_skia::PixmapPaint::default(),
            Transform::identity(),
            None,
        );
    }
    // 名字
    let name = "Sidera";
    let nw = fonts.text_width(name, 38.0);
    fonts.draw(
        &mut pm,
        name,
        (fw - nw) / 2.0,
        148.0,
        38.0,
        Color::WHITE,
        Transform::identity(),
    );
    // byline
    let by = "developed by jinyicheng";
    let bw = fonts.text_width(by, 12.0);
    fonts.draw(
        &mut pm,
        by,
        fw - bw - 20.0,
        fh * 0.78,
        12.0,
        Color::from_rgba8(255, 255, 255, 210),
        Transform::identity(),
    );
    // 进度条
    let bx = 24.0;
    let byy = fh - 30.0;
    let bw2 = fw - 48.0;
    let bh = 10.0;
    if let Some(p) = rounded(bx, byy, bw2, bh, 5.0) {
        pm.fill_path(
            &p,
            &paint_shader(Shader::SolidColor(Color::from_rgba8(255, 255, 255, 45))),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
    let pw = bw2 * progress as f32 / 100.0;
    if pw > 1.0 {
        if let Some(sh) = LinearGradient::new(
            Point::from_xy(bx, 0.0),
            Point::from_xy(bx + bw2, 0.0),
            vec![
                GradientStop::new(0.0, Color::from_rgba8(0x00, 0xe5, 0xff, 255)),
                GradientStop::new(1.0, Color::from_rgba8(0xff, 0x80, 0xab, 255)),
            ],
            SpreadMode::Pad,
            Transform::identity(),
        ) {
            if let Some(p) = rounded(bx, byy, pw, bh, 5.0) {
                pm.fill_path(&p, &paint_shader(sh), FillRule::Winding, Transform::identity(), None);
            }
        }
    }
    pm
}

pub fn show(x11: &X11, fonts: &Fonts, icon: Option<&Pixmap>, dur_ms: u64) {
    let w = 460;
    let h = 210;
    let x = (x11.width - w) / 2;
    let y = (x11.height - h) / 2;
    let Ok((win, gc)) = x11.create_window(x, y, w, h) else {
        return;
    };
    let start = Instant::now();
    loop {
        let el = start.elapsed().as_millis() as u64;
        let p = if dur_ms == 0 { 100 } else { (el * 100 / dur_ms).min(100) };
        let pm = draw(fonts, icon, w as u32, h as u32, p as u32);
        let _ = x11.put_pixmap(win, gc, &pm, 0, 0);
        if el >= dur_ms {
            break;
        }
        std::thread::sleep(Duration::from_millis(16));
    }
    x11.destroy_window(win);
}
