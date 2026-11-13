//! Time glyphs: clock, bell, timer and calendar.

use std::f32::consts::PI;

use egui::{Rect, Shape};

use super::Glyph;
use super::pen::Pen;
use crate::canvas::Canvas;

pub(super) fn paint(canvas: &mut Canvas, g: Glyph, pen: &Pen) {
    let Pen { center, color, size: s, stroke, .. } = *pen;
    let p = |x: f32, y: f32| pen.p(x, y);
    match g {
        Glyph::Clock => {
            canvas.ring(center, 0.42 * s, stroke);
            canvas.add(Shape::line(vec![p(0.0, -0.24), p(0.0, 0.0), p(0.18, 0.1)], stroke));
        }
        Glyph::Bell => {
            let mut dome = pen.arc(0.0, -0.1, 0.26, PI, 2.0 * PI);
            dome.insert(0, p(-0.3, 0.24));
            dome.push(p(0.3, 0.24));
            dome.push(p(-0.3, 0.24));
            canvas.add(Shape::line(dome, stroke));
            canvas.circle(p(0.0, 0.36), 0.07 * s, color);
        }
        Glyph::Timer => {
            canvas.ring(p(0.0, 0.08), 0.36 * s, stroke);
            canvas.add(Shape::line_segment([p(-0.12, -0.44), p(0.12, -0.44)], stroke));
            canvas.add(Shape::line_segment([p(0.0, -0.44), p(0.0, -0.28)], stroke));
            canvas.add(Shape::line_segment([p(0.0, 0.08), p(0.0, -0.12)], stroke));
            canvas.add(Shape::line_segment([p(0.28, -0.26), p(0.36, -0.34)], stroke));
        }
        Glyph::Calendar => {
            let page = Rect::from_min_max(p(-0.42, -0.34), p(0.42, 0.4));
            canvas.rect_stroke(page, 0.1 * s, stroke);
            canvas.rect(Rect::from_min_max(p(-0.42, -0.34), p(0.42, -0.12)), 0.1 * s, color);
            for x in [-0.2, 0.2] {
                canvas.add(Shape::line_segment([p(x, -0.46), p(x, -0.26)], stroke));
            }
            for (x, y) in [(-0.2, 0.04), (0.0, 0.04), (0.2, 0.04), (-0.2, 0.22), (0.0, 0.22)] {
                canvas.circle(p(x, y), 0.055 * s, color);
            }
        }
        _ => {}
    }
}
