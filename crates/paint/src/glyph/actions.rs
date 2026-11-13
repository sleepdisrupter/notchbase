//! File action glyphs: tray, convert, compress, combine, split, gear and folder.

use std::f32::consts::FRAC_PI_4;

use egui::{Rect, Shape, Stroke, vec2};

use super::Glyph;
use super::pen::{Pen, arrow};
use crate::canvas::Canvas;

const GEAR_TEETH: usize = 8;

pub(super) fn paint(canvas: &mut Canvas, g: Glyph, pen: &Pen) {
    let Pen { center, color, size: s, stroke, .. } = *pen;
    let p = |x: f32, y: f32| pen.p(x, y);
    match g {
        Glyph::Tray => {
            let tray = vec![
                p(-0.44, 0.02),
                p(-0.44, 0.40),
                p(0.44, 0.40),
                p(0.44, 0.02),
                p(0.18, 0.02),
                p(0.12, 0.16),
                p(-0.12, 0.16),
                p(-0.18, 0.02),
                p(-0.44, 0.02),
            ];
            canvas.add(Shape::line(tray, stroke));
            canvas.add(Shape::line_segment([p(0.0, -0.46), p(0.0, 0.02)], stroke));
            canvas.add(Shape::line(vec![p(-0.17, -0.14), p(0.0, 0.03), p(0.17, -0.14)], stroke));
        }
        Glyph::Convert => {
            arrow(canvas, p(-0.38, -0.15), p(0.38, -0.15), 0.15 * s, stroke);
            arrow(canvas, p(0.38, 0.15), p(-0.38, 0.15), 0.15 * s, stroke);
        }
        Glyph::Compress => {
            arrow(canvas, p(-0.4, -0.4), p(-0.07, -0.07), 0.2 * s, stroke);
            arrow(canvas, p(0.4, 0.4), p(0.07, 0.07), 0.2 * s, stroke);
        }
        Glyph::Combine => {
            let front = Rect::from_center_size(p(0.1, 0.1), vec2(0.54, 0.54) * s);
            canvas.rect_stroke(front, 0.1 * s, stroke);
            let back = vec![p(0.17, -0.19), p(0.17, -0.37), p(-0.37, -0.37), p(-0.37, 0.17), p(-0.19, 0.17)];
            canvas.add(Shape::line(back, stroke));
        }
        Glyph::Gear => {
            canvas.ring(center, 0.25 * s, Stroke::new(0.13 * s, color));
            let teeth = Stroke::new(0.14 * s, color);
            for i in 0..GEAR_TEETH {
                let angle = i as f32 * FRAC_PI_4;
                let (sin, cos) = angle.sin_cos();
                canvas.add(Shape::line_segment([p(0.28 * cos, 0.28 * sin), p(0.43 * cos, 0.43 * sin)], teeth));
            }
        }
        Glyph::Split => {
            for x in [-0.2, 0.2] {
                canvas.rect_stroke(Rect::from_center_size(p(x, 0.0), vec2(0.3, 0.7) * s), 0.07 * s, stroke);
            }
        }
        Glyph::Folder => {
            let outline = vec![
                p(-0.44, -0.3),
                p(-0.12, -0.3),
                p(0.0, -0.18),
                p(0.44, -0.18),
                p(0.44, 0.34),
                p(-0.44, 0.34),
                p(-0.44, -0.3),
            ];
            canvas.add(Shape::line(outline, stroke));
            canvas.add(Shape::line_segment([p(-0.44, -0.06), p(0.44, -0.06)], stroke));
        }
        _ => {}
    }
}
