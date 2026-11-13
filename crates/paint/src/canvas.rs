//! A frame's shapes, kept together so they can be scaled, moved, faded and clipped as one before they reach egui.

use std::sync::Arc;

use egui::emath::TSTransform;
use egui::{Align2, Color32, CornerRadius, FontId, Mesh, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, pos2};

pub struct Canvas {
    painter: Painter,
    pub shapes: Vec<Shape>,
    clips: Vec<Clip>,
}

struct Clip {
    shapes: std::ops::Range<usize>,
    rect: Rect,
}

#[derive(Clone, Copy, Debug)]
pub struct Mark(usize);

impl Canvas {
    pub fn new(painter: &Painter) -> Self {
        Self { painter: painter.clone(), shapes: Vec::new(), clips: Vec::new() }
    }

    pub fn painter(&self) -> &Painter {
        &self.painter
    }

    pub fn pixels_per_point(&self) -> f32 {
        self.painter.pixels_per_point()
    }

    pub fn mark(&self) -> Mark {
        Mark(self.shapes.len())
    }

    pub fn since(&mut self, mark: Mark) -> &mut [Shape] {
        &mut self.shapes[mark.0..]
    }

    /// Clips every shape added since `mark` to `rect`. Nested clips intersect.
    pub fn clip(&mut self, mark: Mark, rect: Rect) {
        if mark.0 < self.shapes.len() {
            self.clips.push(Clip { shapes: mark.0..self.shapes.len(), rect });
        }
    }

    fn clip_of(&self, shape: usize) -> Option<Rect> {
        let around = self.clips.iter().filter(|clip| clip.shapes.contains(&shape)).map(|clip| clip.rect);
        around.reduce(|inner, outer| inner.intersect(outer))
    }

    pub fn add(&mut self, shape: impl Into<Shape>) {
        self.shapes.push(shape.into());
    }

    pub fn rect(&mut self, rect: Rect, radius: f32, color: Color32) {
        self.add(Shape::rect_filled(rect, CornerRadius::same(radius.round().clamp(0.0, 255.0) as u8), color));
    }

    pub fn rect_stroke(&mut self, rect: Rect, radius: f32, stroke: Stroke) {
        self.add(Shape::rect_stroke(
            rect,
            CornerRadius::same(radius.round().clamp(0.0, 255.0) as u8),
            stroke,
            StrokeKind::Inside,
        ));
    }

    pub fn vertical_gradient(&mut self, rect: Rect, stops: &[Color32]) {
        let Some(last) = stops.len().checked_sub(1).filter(|last| *last > 0) else { return };
        let mut mesh = Mesh::default();
        for (i, color) in stops.iter().enumerate() {
            let y = rect.top() + rect.height() * i as f32 / last as f32;
            mesh.colored_vertex(pos2(rect.left(), y), *color);
            mesh.colored_vertex(pos2(rect.right(), y), *color);
        }
        for band in 0..last as u32 {
            let top = band * 2;
            mesh.add_triangle(top, top + 1, top + 2);
            mesh.add_triangle(top + 1, top + 3, top + 2);
        }
        self.add(Shape::mesh(mesh));
    }

    pub fn circle(&mut self, center: Pos2, radius: f32, color: Color32) {
        self.add(Shape::circle_filled(center, radius, color));
    }

    pub fn ring(&mut self, center: Pos2, radius: f32, stroke: Stroke) {
        self.add(Shape::circle_stroke(center, radius, stroke));
    }

    pub fn text(&mut self, pos: Pos2, anchor: Align2, text: impl Into<String>, font: FontId, color: Color32) -> Rect {
        let galley = self.painter.layout_no_wrap(text.into(), font, color);
        let rect = anchor.anchor_size(pos, galley.size());
        self.add(Shape::galley(rect.min, galley, color));
        rect
    }

    pub fn text_width(&self, text: impl Into<String>, font: FontId) -> f32 {
        self.painter.layout_no_wrap(text.into(), font, Color32::WHITE).size().x
    }

    pub fn text_size(&self, text: impl Into<String>, font: FontId) -> egui::Vec2 {
        self.painter.layout_no_wrap(text.into(), font, Color32::WHITE).size()
    }

    pub fn wrapped(&self, text: impl Into<String>, font: FontId, color: Color32, width: f32) -> Arc<egui::Galley> {
        self.painter.layout(text.into(), font, color, width.max(1.0))
    }

    pub fn clamped(
        &self,
        text: impl Into<String>,
        font: FontId,
        color: Color32,
        width: f32,
        rows: usize,
    ) -> Arc<egui::Galley> {
        let mut job = egui::text::LayoutJob::simple(text.into(), font, color, width.max(1.0));
        job.wrap.max_rows = rows;
        job.wrap.overflow_character = Some('…');
        self.painter.layout_job(job)
    }

    /// Hands the shapes to egui, scaled about `anchor`, then moved by `offset` and faded by `opacity`.
    pub fn paint(self, painter: &Painter, anchor: Pos2, scale: f32, offset: egui::Vec2, opacity: f32) {
        if opacity <= 0.001 {
            return;
        }
        let transform = TSTransform::from_translation(offset) * scale_about(anchor, scale);
        let moved = scale != 1.0 || offset != egui::Vec2::ZERO;
        let mut painter = painter.clone();
        painter.multiply_opacity(opacity);
        let clips: Vec<Option<Rect>> = match self.clips.is_empty() {
            true => Vec::new(),
            false => (0..self.shapes.len()).map(|shape| self.clip_of(shape)).collect(),
        };
        let shapes = self.shapes.into_iter().map(|mut shape| {
            if moved {
                shape.transform(transform);
            }
            shape
        });
        if clips.is_empty() {
            painter.extend(shapes);
            return;
        }
        for (shape, clip) in shapes.zip(clips) {
            match clip {
                Some(rect) => {
                    let rect = if moved { transform * rect } else { rect };
                    painter.with_clip_rect(painter.clip_rect().intersect(rect)).add(shape);
                }
                None => {
                    painter.add(shape);
                }
            }
        }
    }
}

pub fn scale_about(center: Pos2, scale: f32) -> TSTransform {
    TSTransform::from_translation(center.to_vec2())
        * TSTransform::from_scaling(scale)
        * TSTransform::from_translation(-center.to_vec2())
}

pub fn fade(shape: &mut Shape, alpha: f32) {
    if alpha >= 0.999 {
        return;
    }
    let f = |c: &mut Color32| *c = c.gamma_multiply(alpha);
    match shape {
        Shape::Vec(shapes) => shapes.iter_mut().for_each(|s| fade(s, alpha)),
        Shape::Circle(c) => {
            f(&mut c.fill);
            f(&mut c.stroke.color);
        }
        Shape::Ellipse(e) => {
            f(&mut e.fill);
            f(&mut e.stroke.color);
        }
        Shape::LineSegment { stroke, .. } => f(&mut stroke.color),
        Shape::Path(p) => {
            f(&mut p.fill);
            if let egui::epaint::ColorMode::Solid(c) = &mut p.stroke.color {
                f(c);
            }
        }
        Shape::Rect(r) => {
            f(&mut r.fill);
            f(&mut r.stroke.color);
        }
        Shape::Text(t) => t.opacity_factor *= alpha,
        Shape::Mesh(mesh) => Arc::make_mut(mesh).vertices.iter_mut().for_each(|v| f(&mut v.color)),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gradient_spans_rect_band_by_band() {
        let painter = Painter::new(egui::Context::default(), egui::LayerId::background(), Rect::EVERYTHING);
        let mut canvas = Canvas::new(&painter);
        let rect = Rect::from_min_max(pos2(10.0, 20.0), pos2(110.0, 44.0));
        let stops = [Color32::BLACK, Color32::from_black_alpha(128), Color32::TRANSPARENT];
        canvas.vertical_gradient(rect, &stops);
        let Shape::Mesh(mesh) = &canvas.shapes[0] else { panic!("a mesh") };
        assert_eq!((mesh.vertices.len(), mesh.indices.len()), (6, 12));
        assert_eq!(mesh.calc_bounds(), rect);
        assert_eq!((mesh.vertices[0].color, mesh.vertices[5].color), (Color32::BLACK, Color32::TRANSPARENT));
        canvas.vertical_gradient(rect, &stops[..1]);
        assert_eq!(canvas.shapes.len(), 1, "one stop is no gradient");
    }

    #[test]
    fn clip_covers_shapes_since_mark() {
        let painter = Painter::new(egui::Context::default(), egui::LayerId::background(), Rect::EVERYTHING);
        let mut canvas = Canvas::new(&painter);
        let (wide, narrow) = (
            Rect::from_min_max(pos2(0.0, 0.0), pos2(100.0, 20.0)),
            Rect::from_min_max(pos2(40.0, 0.0), pos2(60.0, 50.0)),
        );
        canvas.circle(pos2(5.0, 5.0), 2.0, Color32::WHITE);
        let outer = canvas.mark();
        canvas.circle(pos2(5.0, 5.0), 2.0, Color32::WHITE);
        let inner = canvas.mark();
        canvas.circle(pos2(5.0, 5.0), 2.0, Color32::WHITE);
        canvas.clip(inner, narrow);
        canvas.clip(canvas.mark(), narrow);
        canvas.clip(outer, wide);
        assert_eq!(canvas.clips.len(), 2, "a clip around nothing is dropped");
        assert_eq!([canvas.clip_of(0), canvas.clip_of(1)], [None, Some(wide)]);
        assert_eq!(canvas.clip_of(2), Some(Rect::from_min_max(pos2(40.0, 0.0), pos2(60.0, 20.0))));
    }
}
