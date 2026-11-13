//! Sound and playback glyphs.

use std::f32::consts::{FRAC_PI_4, PI};

use egui::{Rect, Shape, Stroke, vec2};

use super::Glyph;
use super::pen::Pen;
use crate::canvas::Canvas;

pub(super) fn paint(canvas: &mut Canvas, g: Glyph, pen: &Pen) {
    let Pen { color, size: s, stroke, .. } = *pen;
    let p = |x: f32, y: f32| pen.p(x, y);
    match g {
        Glyph::Speaker | Glyph::SpeakerMuted => {
            let body =
                [p(-0.46, -0.16), p(-0.22, -0.16), p(0.04, -0.40), p(0.04, 0.40), p(-0.22, 0.16), p(-0.46, 0.16)];
            pen.fill(canvas, &body, p(-0.12, 0.0));
            if g == Glyph::SpeakerMuted {
                canvas.add(Shape::line_segment([p(0.18, -0.16), p(0.46, 0.16)], stroke));
                canvas.add(Shape::line_segment([p(0.18, 0.16), p(0.46, -0.16)], stroke));
            } else {
                canvas.add(Shape::line(pen.arc(0.04, 0.0, 0.22, -FRAC_PI_4, FRAC_PI_4), stroke));
                canvas.add(Shape::line(pen.arc(0.04, 0.0, 0.40, -FRAC_PI_4, FRAC_PI_4), stroke));
            }
        }
        Glyph::Headphones => {
            canvas.add(Shape::line(pen.arc(0.0, 0.05, 0.38, PI, 2.0 * PI), stroke));
            for x in [-0.38, 0.38] {
                let cup = Rect::from_center_size(p(x, 0.18), vec2(0.18, 0.34) * s);
                canvas.rect(cup, 0.06 * s, color);
            }
        }
        Glyph::Microphone | Glyph::MicrophoneMuted => {
            let capsule = Rect::from_center_size(p(0.0, -0.14), vec2(0.28, 0.5) * s);
            canvas.rect(capsule, 0.14 * s, color);
            canvas.add(Shape::line(pen.arc(0.0, -0.08, 0.28, 0.0, PI), stroke));
            canvas.add(Shape::line_segment([p(0.0, 0.2), p(0.0, 0.42)], stroke));
            if g == Glyph::MicrophoneMuted {
                canvas.add(Shape::line_segment([p(-0.4, -0.42), p(0.4, 0.42)], stroke));
            }
        }
        Glyph::Note => {
            let stem = Stroke::new(stroke.width * 1.1, color);
            canvas.add(Shape::line_segment([p(-0.12, 0.28), p(-0.12, -0.34)], stem));
            canvas.add(Shape::line_segment([p(0.32, 0.18), p(0.32, -0.44)], stem));
            let flag = [p(-0.12, -0.34), p(0.32, -0.44), p(0.32, -0.28), p(-0.12, -0.18)];
            pen.fill(canvas, &flag, p(0.1, -0.31));
            canvas.circle(p(-0.24, 0.3), 0.14 * s, color);
            canvas.circle(p(0.2, 0.2), 0.14 * s, color);
        }
        Glyph::Play => {
            let triangle = [p(-0.3, -0.42), p(0.44, 0.0), p(-0.3, 0.42)];
            pen.fill(canvas, &triangle, p(-0.05, 0.0));
        }
        Glyph::Pause => {
            for x in [-0.19, 0.19] {
                let bar = Rect::from_center_size(p(x, 0.0), vec2(0.22, 0.8) * s);
                canvas.rect(bar, 0.06 * s, color);
            }
        }
        Glyph::Backward | Glyph::Forward => {
            let way = if g == Glyph::Forward { 1.0 } else { -1.0 };
            for start in [-0.48, 0.0] {
                let tip = start + 0.48;
                let (a, b) = (p(way * start, -0.3), p(way * start, 0.3));
                let triangle = [a, p(way * tip, 0.0), b];
                pen.fill(canvas, &triangle, p(way * (start + 0.16), 0.0));
            }
        }
        _ => {}
    }
}
