//! The pen every glyph draws with: offsets in units of the glyph's size, around its center.

use egui::{Color32, Pos2, Shape, Stroke, Vec2, vec2};

use crate::canvas::Canvas;
use crate::shape;

const ARC_STEPS: usize = 16;
const STROKE_SHARE: f32 = 0.09;
const THINNEST: f32 = 1.2;
const HEAD_SPREAD: f32 = 0.8;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Pen {
    pub(super) center: Pos2,
    pub(super) size: f32,
    pub(super) color: Color32,
    pub(super) stroke: Stroke,
    pixels_per_point: f32,
}

impl Pen {
    pub(super) fn new(canvas: &Canvas, center: Pos2, size: f32, color: Color32) -> Self {
        let stroke = Stroke::new(stroke_width(size), color);
        Self { center, size, color, stroke, pixels_per_point: canvas.pixels_per_point() }
    }

    pub(super) fn p(&self, x: f32, y: f32) -> Pos2 {
        self.at(vec2(x, y))
    }

    pub(super) fn at(&self, offset: Vec2) -> Pos2 {
        self.center + offset * self.size
    }

    pub(super) fn arc(&self, cx: f32, cy: f32, r: f32, a0: f32, a1: f32) -> Vec<Pos2> {
        (0..=ARC_STEPS)
            .map(|i| {
                let a = a0 + (a1 - a0) * i as f32 / ARC_STEPS as f32;
                self.p(cx + r * a.cos(), cy + r * a.sin())
            })
            .collect()
    }

    pub(super) fn fill(&self, canvas: &mut Canvas, points: &[Pos2], origin: Pos2) {
        canvas.add(Shape::mesh(shape::fill_polygon(points, origin, self.color, self.pixels_per_point)));
    }
}

pub(super) fn stroke_width(size: f32) -> f32 {
    (size * STROKE_SHARE).max(THINNEST)
}

pub(super) fn arrow(canvas: &mut Canvas, from: Pos2, to: Pos2, head: f32, stroke: Stroke) {
    canvas.add(Shape::line_segment([from, to], stroke));
    arrow_head(canvas, to, (to - from).normalized(), head, stroke);
}

pub(super) fn arrow_head(canvas: &mut Canvas, tip: Pos2, heading: Vec2, size: f32, stroke: Stroke) {
    let back = tip - heading * size;
    let side = vec2(-heading.y, heading.x) * size * HEAD_SPREAD;
    canvas.add(Shape::line(vec![back + side, tip, back - side], stroke));
}

#[cfg(test)]
mod tests {
    use egui::{LayerId, Painter, Rect, pos2};

    use super::*;

    fn canvas() -> Canvas {
        Canvas::new(&Painter::new(egui::Context::default(), LayerId::background(), Rect::EVERYTHING))
    }

    #[test]
    fn pen_maps_unit_offsets() {
        let pen = Pen::new(&canvas(), pos2(50.0, 40.0), 20.0, Color32::WHITE);
        assert_eq!(pen.p(0.0, 0.0), pos2(50.0, 40.0));
        assert_eq!(pen.p(0.5, -0.25), pos2(60.0, 35.0));
        assert_eq!(pen.at(vec2(-1.0, 1.0)), pos2(30.0, 60.0));
    }

    #[test]
    fn stroke_has_a_minimum_width() {
        assert_eq!(stroke_width(5.0), THINNEST);
        assert_eq!(stroke_width(0.0), THINNEST);
        assert!((stroke_width(40.0) - 3.6).abs() < 1e-6);
        let pen = Pen::new(&canvas(), Pos2::ZERO, 40.0, Color32::RED);
        assert_eq!(pen.stroke, Stroke::new(stroke_width(40.0), Color32::RED));
    }

    #[test]
    fn arc_takes_even_steps() {
        let pen = Pen::new(&canvas(), Pos2::ZERO, 10.0, Color32::WHITE);
        let half = pen.arc(0.0, 0.0, 1.0, 0.0, std::f32::consts::PI);
        assert_eq!(half.len(), ARC_STEPS + 1);
        assert!((half[0] - pos2(10.0, 0.0)).length() < 1e-4);
        assert!((half[ARC_STEPS / 2] - pos2(0.0, 10.0)).length() < 1e-4);
        assert!((half[ARC_STEPS] - pos2(-10.0, 0.0)).length() < 1e-4);
        assert!(half.iter().all(|point| (point.to_vec2().length() - 10.0).abs() < 1e-4));
    }

    #[test]
    fn fill_adds_one_mesh() {
        let points = [pos2(0.0, 0.0), pos2(10.0, 0.0), pos2(0.0, 10.0)];
        let mut canvas = canvas();
        let pen = Pen::new(&canvas, Pos2::ZERO, 10.0, Color32::GREEN);
        assert_eq!(pen.pixels_per_point, canvas.pixels_per_point());
        pen.fill(&mut canvas, &points, pos2(2.0, 2.0));
        let expected = shape::fill_polygon(&points, pos2(2.0, 2.0), Color32::GREEN, canvas.pixels_per_point());
        assert!(matches!(&canvas.shapes[..], [Shape::Mesh(mesh)] if **mesh == expected));
    }

    #[test]
    fn arrow_is_shaft_and_head() {
        let mut canvas = canvas();
        let stroke = Stroke::new(2.0, Color32::WHITE);
        arrow(&mut canvas, pos2(0.0, 0.0), pos2(10.0, 0.0), 4.0, stroke);
        let [Shape::LineSegment { points, .. }, Shape::Path(head)] = &canvas.shapes[..] else {
            panic!("a shaft and a head")
        };
        assert_eq!(*points, [pos2(0.0, 0.0), pos2(10.0, 0.0)]);
        assert_eq!(head.points, [pos2(6.0, 3.2), pos2(10.0, 0.0), pos2(6.0, -3.2)]);
    }
}
