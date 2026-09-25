// ============================================================
// Sidera - 笔迹绘制（画笔/橡皮，纯 tiny-skia 光栅）
// 与原 C++ drawing.cpp 语义一致：画笔 SourceOver、橡皮 Clear
// ============================================================
use tiny_skia::{BlendMode, Color, FillRule, LineCap, LineJoin, Paint, PathBuilder, Pixmap, Shader, Stroke, Transform};

use crate::app::App;

fn solid(color: Color, blend: BlendMode) -> Paint<'static> {
    Paint {
        shader: Shader::SolidColor(color),
        anti_alias: true,
        blend_mode: blend,
        ..Default::default()
    }
}

pub fn pen_segment(canvas: &mut Pixmap, color: Color, a: (i32, i32), b: (i32, i32), width: i32) {
    let paint = solid(color, BlendMode::SourceOver);
    if a == b {
        let mut pb = PathBuilder::new();
        pb.push_circle(a.0 as f32, a.1 as f32, width as f32 / 2.0);
        if let Some(p) = pb.finish() {
            canvas.fill_path(&p, &paint, FillRule::Winding, Transform::identity(), None);
        }
    } else {
        let stroke = Stroke {
            width: width as f32,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            ..Default::default()
        };
        let mut pb = PathBuilder::new();
        pb.move_to(a.0 as f32, a.1 as f32);
        pb.line_to(b.0 as f32, b.1 as f32);
        if let Some(p) = pb.finish() {
            canvas.stroke_path(&p, &paint, &stroke, Transform::identity(), None);
        }
    }
}

pub fn erase_segment(canvas: &mut Pixmap, a: (i32, i32), b: (i32, i32), width: i32) {
    let paint = solid(Color::TRANSPARENT, BlendMode::Clear);
    if a == b {
        let mut pb = PathBuilder::new();
        pb.push_circle(a.0 as f32, a.1 as f32, width as f32 / 2.0);
        if let Some(p) = pb.finish() {
            canvas.fill_path(&p, &paint, FillRule::Winding, Transform::identity(), None);
        }
    } else {
        let stroke = Stroke {
            width: width as f32,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            ..Default::default()
        };
        let mut pb = PathBuilder::new();
        pb.move_to(a.0 as f32, a.1 as f32);
        pb.line_to(b.0 as f32, b.1 as f32);
        if let Some(p) = pb.finish() {
            canvas.stroke_path(&p, &paint, &stroke, Transform::identity(), None);
        }
    }
}

pub fn pen_tapered(canvas: &mut Pixmap, color: Color, a: (i32, i32), b: (i32, i32), w0: i32, w1: i32) {
    if a == b {
        pen_segment(canvas, color, a, b, w0.max(1));
        return;
    }
    let len = (b.0 - a.0).abs() + (b.1 - a.1).abs();
    let steps = (len / 4 + 1).clamp(2, 16);
    for i in 0..steps {
        let t0 = i as f64 / steps as f64;
        let t1 = (i + 1) as f64 / steps as f64;
        let p0 = (
            (a.0 as f64 + (b.0 - a.0) as f64 * t0).round() as i32,
            (a.1 as f64 + (b.1 - a.1) as f64 * t0).round() as i32,
        );
        let p1 = (
            (a.0 as f64 + (b.0 - a.0) as f64 * t1).round() as i32,
            (a.1 as f64 + (b.1 - a.1) as f64 * t1).round() as i32,
        );
        let w = (w0 as f64 + (w1 - w0) as f64 * ((t0 + t1) / 2.0)).round() as i32;
        pen_segment(canvas, color, p0, p1, w.max(1));
    }
}

// ---- 带全局状态的操作 ----
pub fn stroke_segment(app: &mut App, a: (i32, i32), b: (i32, i32), erase: bool, width: i32) {
    if erase {
        erase_segment(&mut app.canvas, a, b, width);
    } else {
        app.page_has_ink = true;
        let c = app.pen_color();
        pen_segment(&mut app.canvas, c, a, b, width);
    }
}

pub fn stroke_tapered(app: &mut App, a: (i32, i32), b: (i32, i32), w0: i32, w1: i32) {
    app.page_has_ink = true;
    let c = app.pen_color();
    pen_tapered(&mut app.canvas, c, a, b, w0, w1);
}
