//! Classroom tools drawn on a shared 24-unit grid, independent of system fonts.
use tiny_skia::{Color, FillRule, LineCap, LineJoin, Paint, Path, PathBuilder, Shader, Stroke};
use crate::ui::Ctx;
use crate::ui_theme::{self as theme, c};

#[derive(Clone, Copy)]
pub enum Glyph { Cursor, Pen, Eraser, Up, Down, More, Camera, Settings, Check, Info, Document, Right }

fn paint(ink: Color) -> Paint<'static> {
    Paint { shader: Shader::SolidColor(ink), anti_alias: true, ..Default::default() }
}

fn path(ctx: &mut Ctx, p: Option<Path>, fill: Option<Color>, outline: Option<Color>) {
    let Some(p) = p else { return };
    if let Some(fill) = fill {
        ctx.pm.fill_path(&p, &paint(fill), FillRule::Winding, ctx.tr, None);
    }
    if let Some(outline) = outline {
        ctx.pm.stroke_path(&p, &paint(outline), &Stroke {
            width: 1.8, line_cap: LineCap::Round, line_join: LineJoin::Round,
            ..Default::default()
        }, ctx.tr, None);
    }
}

fn polygon(points: &[(f32, f32)]) -> Option<Path> {
    let mut p = PathBuilder::new();
    p.move_to(points[0].0, points[0].1);
    for &(x, y) in &points[1..] { p.line_to(x, y); }
    p.close();
    p.finish()
}

pub fn draw(ctx: &mut Ctx, glyph: Glyph, x: f32, y: f32, size: f32, ink: Color, accent: Color) {
    let tr = ctx.tr.pre_translate(x, y).pre_scale(size / 24.0, size / 24.0);
    let mut g = Ctx { pm: &mut *ctx.pm, tr };
    let line = |g: &mut Ctx, a, b| g.stroke_line(a, b, ink, 1.8);
    match glyph {
        Glyph::Cursor => {
            path(&mut g, polygon(&[(5.0, 3.0), (19.0, 12.0), (12.5, 13.2),
                (16.0, 19.3), (12.7, 21.0), (9.4, 14.8), (5.0, 19.0)]),
                Some(c(theme::PAPER)), Some(ink));
        }
        Glyph::Pen => {
            let mut p = PathBuilder::new();
            p.move_to(3.8, 20.2);
            p.line_to(5.3, 14.6);
            p.line_to(15.9, 4.0);
            p.quad_to(17.6, 2.3, 19.3, 4.0);
            p.line_to(20.0, 4.7);
            p.quad_to(21.7, 6.4, 20.0, 8.1);
            p.line_to(9.4, 18.7);
            p.close();
            let outline = p.finish();
            path(&mut g, outline.clone(), Some(c(theme::PAPER)), None);
            path(&mut g, polygon(&[(14.0, 6.0), (16.7, 3.5), (18.3, 3.5),
                (20.5, 5.7), (20.5, 7.3), (18.0, 10.0)]), Some(c(theme::ACCENT_LIGHT)), None);
            // Keep enough coloured area at normal toolbar size; an outlined
            // triangle alone collapses to a dark point at low pixel density.
            path(&mut g, polygon(&[(3.8, 20.2), (5.3, 14.6), (7.5, 12.4),
                (11.6, 16.5), (9.4, 18.7)]),
                Some(accent), None);
            path(&mut g, outline, None, Some(ink));
            g.stroke_line((7.5, 12.4), (11.6, 16.5), ink, 1.3);
            line(&mut g, (14.0, 6.0), (18.0, 10.0));
            g.stroke_line((10.3, 12.1), (13.4, 9.0), ink, 1.1);
        }
        Glyph::Eraser => {
            let mut p = PathBuilder::new();
            p.move_to(4.2, 12.2);
            p.line_to(12.0, 4.4);
            p.quad_to(13.6, 2.8, 15.2, 4.4);
            p.line_to(20.6, 9.8);
            p.quad_to(22.2, 11.4, 20.6, 13.0);
            p.line_to(13.6, 20.0);
            p.line_to(8.0, 20.0);
            p.line_to(4.2, 16.2);
            p.quad_to(2.6, 14.2, 4.2, 12.2);
            p.close();
            let outline = p.finish();
            path(&mut g, outline.clone(), Some(c(theme::ERASER)), None);
            let mut p = PathBuilder::new();
            p.move_to(7.5, 8.9); p.line_to(17.1, 16.5); p.line_to(13.6, 20.0);
            p.line_to(8.0, 20.0); p.line_to(4.2, 16.2);
            p.quad_to(2.6, 14.2, 4.2, 12.2); p.close();
            path(&mut g, p.finish(), Some(c(theme::PAPER)), None);
            path(&mut g, outline, None, Some(ink));
            line(&mut g, (7.5, 8.9), (17.1, 16.5));
            line(&mut g, (17.0, 20.0), (22.0, 20.0));
        }
        Glyph::Up | Glyph::Down => {
            let (outer, center) = if matches!(glyph, Glyph::Up) { (14.5, 8.5) } else { (9.5, 15.5) };
            line(&mut g, (6.0, outer), (12.0, center));
            line(&mut g, (12.0, center), (18.0, outer));
        }
        Glyph::More => {
            for x in [5.0, 12.0, 19.0] { g.fill_circle(x, 12.0, 1.65, ink); }
        }
        Glyph::Camera => {
            let mut p = PathBuilder::new();
            p.move_to(5.0, 7.0); p.line_to(7.0, 7.0); p.line_to(9.0, 4.5);
            p.line_to(15.0, 4.5); p.line_to(17.0, 7.0); p.line_to(19.0, 7.0);
            p.quad_to(21.0, 7.0, 21.0, 9.0); p.line_to(21.0, 18.0);
            p.quad_to(21.0, 20.0, 19.0, 20.0); p.line_to(5.0, 20.0);
            p.quad_to(3.0, 20.0, 3.0, 18.0); p.line_to(3.0, 9.0);
            p.quad_to(3.0, 7.0, 5.0, 7.0); p.close();
            path(&mut g, p.finish(), None, Some(ink));
            let mut p = PathBuilder::new(); p.push_circle(12.0, 13.0, 3.5);
            path(&mut g, p.finish(), None, Some(ink));
        }
        Glyph::Settings => {
            for (y, x) in [(6.0, 9.0), (12.0, 15.0), (18.0, 9.0)] {
                line(&mut g, (3.0, y), (21.0, y));
                let mut p = PathBuilder::new(); p.push_circle(x, y, 2.2);
                path(&mut g, p.finish(), Some(c(theme::PAPER)), Some(ink));
            }
        }
        Glyph::Check => {
            line(&mut g, (5.0, 12.0), (10.0, 17.0));
            line(&mut g, (10.0, 17.0), (19.0, 7.0));
        }
        Glyph::Info => {
            let mut p = PathBuilder::new(); p.push_circle(12.0, 12.0, 9.0);
            path(&mut g, p.finish(), None, Some(ink));
            g.fill_circle(12.0, 7.5, 1.1, ink);
            line(&mut g, (12.0, 11.0), (12.0, 17.0));
        }
        Glyph::Document => {
            path(&mut g, polygon(&[(5.0, 3.0), (14.0, 3.0), (19.0, 8.0), (19.0, 21.0), (5.0, 21.0)]),
                None, Some(ink));
            line(&mut g, (14.0, 3.0), (14.0, 8.0));
            line(&mut g, (14.0, 8.0), (19.0, 8.0));
            line(&mut g, (9.0, 12.0), (15.0, 12.0));
            line(&mut g, (9.0, 16.0), (15.0, 16.0));
        }
        Glyph::Right => {
            line(&mut g, (9.0, 6.0), (15.0, 12.0));
            line(&mut g, (15.0, 12.0), (9.0, 18.0));
        }
    }
}
