//! Small interface marks: chevrons, checkmark, x, plus, minus and the exclamation mark.

use egui::{Color32, Pos2, Shape, Stroke, Vec2, vec2};

use super::Glyph;
use super::pen::{Pen, stroke_width};
use crate::canvas::Canvas;

const BOLD: f32 = 1.2;
const EXCLAIMED: f32 = 1.3;
const DOT: f32 = 0.8;

pub(super) fn turning(canvas: &mut Canvas, pen: &Pen, up: f32) {
    let y = |down: f32| down + (-down - down) * up.clamp(0.0, 1.0);
    let pts = vec![pen.p(-0.3, y(-0.12)), pen.p(0.0, y(0.18)), pen.p(0.3, y(-0.12))];
    canvas.add(Shape::line(pts, pen.stroke));
}

pub(super) fn paint(canvas: &mut Canvas, g: Glyph, pen: &Pen) {
    let Pen { center, color, size: s, stroke, .. } = *pen;
    let p = |x: f32, y: f32| pen.p(x, y);
    let w = stroke.width;
    match g {
        Glyph::Checkmark => checkmark(canvas, center, s, w * BOLD, color),
        Glyph::ChevronDown | Glyph::ChevronUp | Glyph::ChevronLeft | Glyph::ChevronRight => {
            let pts = match g {
                Glyph::ChevronDown => vec![p(-0.3, -0.12), p(0.0, 0.18), p(0.3, -0.12)],
                Glyph::ChevronUp => vec![p(-0.3, 0.12), p(0.0, -0.18), p(0.3, 0.12)],
                Glyph::ChevronRight => vec![p(-0.12, -0.3), p(0.18, 0.0), p(-0.12, 0.3)],
                _ => vec![p(0.12, -0.3), p(-0.18, 0.0), p(0.12, 0.3)],
            };
            canvas.add(Shape::line(pts, stroke));
        }
        Glyph::Xmark => {
            canvas.add(Shape::line_segment([p(-0.26, -0.26), p(0.26, 0.26)], stroke));
            canvas.add(Shape::line_segment([p(-0.26, 0.26), p(0.26, -0.26)], stroke));
        }
        Glyph::Exclamation => {
            canvas.add(Shape::line_segment([p(0.0, -0.36), p(0.0, 0.12)], Stroke::new(w * EXCLAIMED, color)));
            canvas.circle(p(0.0, 0.34), w * DOT, color);
        }
        Glyph::Plus | Glyph::Minus => {
            let bold = Stroke::new(w * BOLD, color);
            canvas.add(Shape::line_segment([p(-0.34, 0.0), p(0.34, 0.0)], bold));
            if g == Glyph::Plus {
                canvas.add(Shape::line_segment([p(0.0, -0.34), p(0.0, 0.34)], bold));
            }
        }
        _ => {}
    }
}

pub fn checkmark(canvas: &mut Canvas, center: Pos2, size: f32, width: f32, color: Color32) {
    let p = |x: f32, y: f32| center + vec2(x, y) * size;
    canvas.add(Shape::line(vec![p(-0.36, 0.02), p(-0.11, 0.27), p(0.38, -0.26)], Stroke::new(width, color)));
}

pub fn chevron(canvas: &mut Canvas, center: Pos2, size: f32, turn: f32, color: Color32) {
    let pts = chevron_points(turn).map(|v| center + v * size).to_vec();
    canvas.add(Shape::line(pts, Stroke::new(stroke_width(size), color)));
}

fn chevron_points(turn: f32) -> [Vec2; 3] {
    let (sin, cos) = (turn.clamp(0.0, 1.0) * std::f32::consts::PI).sin_cos();
    [vec2(-0.3, -0.12), vec2(0.0, 0.18), vec2(0.3, -0.12)].map(|v| vec2(v.x * cos - v.y * sin, v.x * sin + v.y * cos))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [Vec2; 3], b: [Vec2; 3]) -> bool {
        a.iter().zip(b).all(|(a, b)| (*a - b).length() < 1e-5)
    }

    #[test]
    fn chevron_turns_down_to_up() {
        assert!(close(chevron_points(0.0), [vec2(-0.3, -0.12), vec2(0.0, 0.18), vec2(0.3, -0.12)]));
        assert!(close(chevron_points(1.0), [vec2(0.3, 0.12), vec2(0.0, -0.18), vec2(-0.3, 0.12)]));
        assert!(close(chevron_points(2.0), chevron_points(1.0)));
    }

    #[test]
    fn negative_turn_stays_down() {
        assert!(close(chevron_points(-3.0), chevron_points(0.0)));
        let half = chevron_points(0.5);
        assert!((half[1] - vec2(-0.18, 0.0)).length() < 1e-5, "{half:?}");
    }
}
