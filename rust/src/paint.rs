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

pub fn pen_segment(canvas: &mut Pixmap, color: Color, a: (i32, i32), b: (i32, i32), width: i32, tf: Transform) {
    let paint = solid(color, BlendMode::SourceOver);
    if a == b {
        let mut pb = PathBuilder::new();
        pb.push_circle(a.0 as f32, a.1 as f32, width as f32 / 2.0);
        if let Some(p) = pb.finish() {
            canvas.fill_path(&p, &paint, FillRule::Winding, tf, None);
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
            canvas.stroke_path(&p, &paint, &stroke, tf, None);
        }
    }
}

pub fn erase_segment(canvas: &mut Pixmap, a: (i32, i32), b: (i32, i32), width: i32, tf: Transform) {
    let paint = solid(Color::TRANSPARENT, BlendMode::Clear);
    if a == b {
        let mut pb = PathBuilder::new();
        pb.push_circle(a.0 as f32, a.1 as f32, width as f32 / 2.0);
        if let Some(p) = pb.finish() {
            canvas.fill_path(&p, &paint, FillRule::Winding, tf, None);
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
            canvas.stroke_path(&p, &paint, &stroke, tf, None);
        }
    }
}

pub fn pen_tapered(canvas: &mut Pixmap, color: Color, a: (i32, i32), b: (i32, i32), w0: i32, w1: i32, tf: Transform) {
    if a == b {
        pen_segment(canvas, color, a, b, w0.max(1), tf);
        return;
    }
    let pts = [
        (a.0 as f32, a.1 as f32, w0 as f32),
        (b.0 as f32, b.1 as f32, w1 as f32),
    ];
    draw_stroke_union(canvas, color, &pts, tf);
}

/// 把一条笔画（采样点 + 每点宽度）构建为**单个并集路径**：
/// 每个采样点的圆（圆角接头/端帽）+ 相邻点之间的梯形。
/// 一次填充，避免多段分别绘制时抗锯齿边缘互相叠加 → 毛边。
pub fn build_stroke_path(pts: &[(f32, f32, f32)]) -> Option<tiny_skia::Path> {
    if pts.is_empty() {
        return None;
    }
    let mut pb = PathBuilder::new();
    let mut any = false;
    if pts.len() == 1 {
        let (x, y, w) = pts[0];
        pb.push_circle(x, y, (w * 0.5).max(0.5));
        any = true;
    } else {
        for i in 0..pts.len() - 1 {
            let (x0, y0, w0) = pts[i];
            let (x1, y1, w1) = pts[i + 1];
            let dx = x1 - x0;
            let dy = y1 - y0;
            let len = (dx * dx + dy * dy).sqrt();
            if len < 0.01 {
                continue;
            }
            let nx = -dy / len;
            let ny = dx / len;
            let h0 = (w0 * 0.5).max(0.5);
            let h1 = (w1 * 0.5).max(0.5);
            // 梯形顶点顺序必须与 push_circle 的绕向一致（顺时针），
            // 否则 Winding 填充时圆与梯形重叠处会互相抵消，弯折处出现断口。
            pb.move_to(x0 - nx * h0, y0 - ny * h0);
            pb.line_to(x1 - nx * h1, y1 - ny * h1);
            pb.line_to(x1 + nx * h1, y1 + ny * h1);
            pb.line_to(x0 + nx * h0, y0 + ny * h0);
            pb.close();
            any = true;
        }
        for &(x, y, w) in pts {
            pb.push_circle(x, y, (w * 0.5).max(0.5));
            any = true;
        }
    }
    if !any {
        return None;
    }
    pb.finish()
}

pub fn draw_stroke_union(
    pm: &mut Pixmap,
    color: Color,
    pts: &[(f32, f32, f32)],
    transform: Transform,
) {
    if let Some(p) = build_stroke_path(pts) {
        pm.fill_path(
            &p,
            &solid(color, BlendMode::SourceOver),
            FillRule::Winding,
            transform,
            None,
        );
    }
}

/// 提交当前进行中的画笔笔画到画布（单次填充 → 边缘干净）
pub fn commit_stroke(app: &mut App) {
    if app.stroke_pts.is_empty() {
        return;
    }
    let c = app.pen_color();
    let tf = Transform::from_scale(app.scale as f32, app.scale as f32);
    if let Some(p) = build_stroke_path(&app.stroke_pts) {
        app.canvas.fill_path(&p, &solid(c, BlendMode::SourceOver), FillRule::Winding, tf, None);
        app.page_has_ink = true;
    }
    app.stroke_pts.clear();
}

// ---- 带全局状态的操作 ----
fn scale_tf(app: &App) -> Transform {
    Transform::from_scale(app.scale as f32, app.scale as f32)
}

pub fn stroke_segment(app: &mut App, a: (i32, i32), b: (i32, i32), erase: bool, width: i32) {
    let tf = scale_tf(app);
    if erase {
        erase_segment(&mut app.canvas, a, b, width, tf);
    } else {
        app.page_has_ink = true;
        let c = app.pen_color();
        pen_segment(&mut app.canvas, c, a, b, width, tf);
    }
}

pub fn stroke_tapered(app: &mut App, a: (i32, i32), b: (i32, i32), w0: i32, w1: i32) {
    app.page_has_ink = true;
    let c = app.pen_color();
    let tf = scale_tf(app);
    pen_tapered(&mut app.canvas, c, a, b, w0, w1, tf);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stroke_union_is_continuous() {
        // 模拟一串鼠标采样点（水平），整条笔画的中心线应处处不透明
        let pts: Vec<(f32, f32, f32)> = (0..40)
            .map(|i| (20.0 + i as f32 * 4.0, 50.0, 8.0))
            .collect();
        let mut pm = Pixmap::new(200, 100).unwrap();
        draw_stroke_union(&mut pm, crate::app::color(0, 0, 0, 255), &pts, Transform::identity());
        let w = pm.width() as usize;
        let mut holes = Vec::new();
        for i in 0..40 {
            let x = (20.0 + i as f32 * 4.0) as usize;
            let a = pm.data()[(50 * w + x) * 4 + 3];
            if a < 200 {
                holes.push((x, a));
            }
        }
        assert!(holes.is_empty(), "笔画在采样点处有空洞: {:?}", holes);
    }
}

#[cfg(test)]
mod tests2 {
    use super::*;

    #[test]
    fn curved_stroke_continuous() {
        let mut pts: Vec<(f32, f32, f32)> = Vec::new();
        for i in 0..220 {
            let t = i as f32 / 219.0;
            let x = 100.0 + t * 600.0;
            let y = 60.0 + (t * 12.0).sin() * 20.0;
            pts.push((x, y, 10.0));
        }
        let mut pm = Pixmap::new(760, 120).unwrap();
        draw_stroke_union(&mut pm, crate::app::color(0, 0, 0, 255), &pts, Transform::identity());
        let w = pm.width() as usize;
        let mut holes = Vec::new();
        for (i, (x, y, _)) in pts.iter().enumerate() {
            let a = pm.data()[((*y as usize) * w + (*x as usize)) * 4 + 3];
            if a < 200 {
                holes.push((i, *x as i32, *y as i32, a));
            }
        }
        eprintln!("holes={}/{} first={:?}", holes.len(), pts.len(), holes.first());
        assert!(holes.is_empty(), "曲线笔迹有空洞");
    }
}
