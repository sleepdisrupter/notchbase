//! Device glyphs: displays, keyboards, mice, gamepads, phones, watches and Bluetooth.

use egui::{Rect, Shape, vec2};

use super::Glyph;
use super::pen::Pen;
use crate::canvas::Canvas;

pub(super) fn paint(canvas: &mut Canvas, g: Glyph, pen: &Pen) {
    let Pen { center, color, size: s, stroke, .. } = *pen;
    let p = |x: f32, y: f32| pen.p(x, y);
    match g {
        Glyph::Display => {
            let screen = Rect::from_center_size(p(0.0, -0.08), vec2(0.86, 0.56) * s);
            canvas.rect_stroke(screen, 0.08 * s, stroke);
            canvas.add(Shape::line_segment([p(-0.18, 0.36), p(0.18, 0.36)], stroke));
            canvas.add(Shape::line_segment([p(0.0, 0.2), p(0.0, 0.36)], stroke));
        }
        Glyph::Device => {
            let body = Rect::from_center_size(center, vec2(0.62, 0.78) * s);
            canvas.rect_stroke(body, 0.12 * s, stroke);
            canvas.ring(p(0.0, 0.1), 0.16 * s, stroke);
            canvas.circle(p(0.0, -0.22), 0.05 * s, color);
        }
        Glyph::Keyboard => {
            let body = Rect::from_center_size(center, vec2(0.92, 0.58) * s);
            canvas.rect_stroke(body, 0.1 * s, stroke);
            for (row, y) in [-0.1, 0.05].into_iter().enumerate() {
                for i in 0..4 {
                    let x = -0.27 + 0.18 * i as f32 + 0.04 * row as f32;
                    canvas.circle(p(x, y), 0.038 * s, color);
                }
            }
            canvas.add(Shape::line_segment([p(-0.18, 0.18), p(0.18, 0.18)], stroke));
        }
        Glyph::Mouse => {
            let body = Rect::from_center_size(center, vec2(0.52, 0.82) * s);
            canvas.rect_stroke(body, 0.26 * s, stroke);
            canvas.add(Shape::line_segment([p(0.0, -0.4), p(0.0, -0.14)], stroke));
        }
        Glyph::Gamepad => {
            let body = Rect::from_center_size(p(0.0, 0.02), vec2(0.96, 0.56) * s);
            canvas.rect_stroke(body, 0.26 * s, stroke);
            canvas.add(Shape::line_segment([p(-0.32, 0.02), p(-0.14, 0.02)], stroke));
            canvas.add(Shape::line_segment([p(-0.23, -0.07), p(-0.23, 0.11)], stroke));
            canvas.circle(p(0.17, -0.03), 0.05 * s, color);
            canvas.circle(p(0.29, 0.08), 0.05 * s, color);
        }
        Glyph::Phone => {
            let body = Rect::from_center_size(center, vec2(0.48, 0.86) * s);
            canvas.rect_stroke(body, 0.1 * s, stroke);
            canvas.add(Shape::line_segment([p(-0.08, -0.31), p(0.08, -0.31)], stroke));
        }
        Glyph::Watch => {
            for (top, bottom) in [(-0.46, -0.26), (0.26, 0.46)] {
                let strap = Rect::from_min_max(p(-0.15, top), p(0.15, bottom));
                canvas.rect(strap, 0.04 * s, color);
            }
            let face = Rect::from_center_size(center, vec2(0.5, 0.56) * s);
            canvas.rect_stroke(face, 0.15 * s, stroke);
        }
        Glyph::Bluetooth => {
            let rune = vec![p(-0.2, -0.18), p(0.2, 0.2), p(0.0, 0.4), p(0.0, -0.4), p(0.2, -0.2), p(-0.2, 0.18)];
            canvas.add(Shape::line(rune, stroke));
        }
        _ => {}
    }
}
