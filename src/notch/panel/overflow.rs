//! Scrolling a page that doesn't fit: the fades at the edges and the scrollbar.

use egui::{Color32, Rect, pos2, vec2};

use crate::ui::canvas::{Canvas, fade};
use crate::ui::color::white;
use crate::ui::metrics::PAD;
use crate::ui::spring::{lerp, smoothstep};

const FADE_H: f32 = 24.0;
const SOLID_FROM: f32 = 0.6;
const FADE_STOPS: usize = 8;
const EDGE_ROOM: f32 = 1.0;
const SCROLLBAR_W: f32 = 3.0;
const SCROLLBAR_INSET: f32 = 2.0;
const SCROLLBAR_MIN: f32 = 24.0;
const SCROLLBAR_ALPHA: f32 = 0.22;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::notch) struct Overflow {
    pub(in crate::notch) window: Rect,
    pub(in crate::notch) natural: f32,
    pub(in crate::notch) offset: f32,
}

impl Overflow {
    pub(in crate::notch) fn spare(&self) -> f32 {
        (self.natural - self.window.height()).max(0.0)
    }

    pub(in crate::notch) fn content(&self) -> Rect {
        Rect::from_min_size(self.window.min - vec2(0.0, self.offset), vec2(self.window.width(), self.natural))
    }

    pub(in crate::notch) fn clip(&self) -> Rect {
        let window = self.window.expand(EDGE_ROOM);
        Rect::from_x_y_ranges(window.left()..=self.window.right() + PAD * 0.5, window.y_range())
    }

    pub(in crate::notch) fn top_fade(&self) -> f32 {
        (self.offset / FADE_H).clamp(0.0, 1.0)
    }

    pub(in crate::notch) fn bottom_fade(&self) -> f32 {
        ((self.spare() - self.offset) / FADE_H).clamp(0.0, 1.0)
    }

    pub(super) fn scrollbar(&self) -> Rect {
        let height = (self.window.height() * self.window.height() / self.natural).max(SCROLLBAR_MIN);
        let travel = self.window.height() - height;
        let top = self.window.top() + travel * self.offset / self.spare().max(1.0);
        let center = self.window.right() + PAD * 0.5 - SCROLLBAR_INSET;
        Rect::from_min_size(pos2(center - SCROLLBAR_W * 0.5, top), vec2(SCROLLBAR_W, height))
    }

    pub(in crate::notch) fn paint(&self, canvas: &mut Canvas, [above, below]: [Color32; 2]) {
        let (top, bottom) = (self.window.top(), self.window.bottom());
        let band = |from: f32, to: f32| Rect::from_x_y_ranges(self.window.x_range(), from..=to);
        if solid(above) {
            canvas.vertical_gradient(band(top, top + FADE_H), &stops(self.top_fade(), true, above));
        } else {
            self.dim_toward(canvas, top, self.top_fade());
        }
        if solid(below) {
            canvas.vertical_gradient(band(bottom - FADE_H, bottom), &stops(self.bottom_fade(), false, below));
        } else {
            self.dim_toward(canvas, bottom, self.bottom_fade());
        }
        canvas.rect(self.scrollbar(), SCROLLBAR_W * 0.5, white(SCROLLBAR_ALPHA));
    }

    fn dim_toward(&self, canvas: &mut Canvas, edge: f32, strength: f32) {
        if strength <= 0.0 {
            return;
        }
        for shape in &mut canvas.shapes {
            let center = shape.visual_bounding_rect().center().y;
            let inside = ((center - edge).abs() / FADE_H).clamp(0.0, 1.0);
            fade(shape, lerp(1.0, smoothstep(0.0, 1.0, inside), strength));
        }
    }
}

fn solid(color: Color32) -> bool {
    f32::from(color.a()) / 255.0 >= SOLID_FROM
}

fn stops(strength: f32, edge_first: bool, body: Color32) -> Vec<Color32> {
    (0..=FADE_STOPS)
        .map(|i| {
            let along = i as f32 / FADE_STOPS as f32;
            let toward_edge = if edge_first { 1.0 - along } else { along };
            body.gamma_multiply(strength * smoothstep(0.0, 1.0, toward_edge))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use egui::{Painter, Shape};

    use super::*;

    const WINDOW: Rect = Rect { min: pos2(16.0, 80.0), max: pos2(416.0, 480.0) };

    fn at(offset: f32) -> Overflow {
        Overflow { window: WINDOW, natural: 600.0, offset }
    }

    #[test]
    fn fades_follow_hidden_content() {
        assert_eq!((at(0.0).top_fade(), at(0.0).bottom_fade()), (0.0, 1.0));
        assert_eq!((at(12.0).top_fade(), at(12.0).bottom_fade()), (0.5, 1.0));
        assert_eq!((at(100.0).top_fade(), at(100.0).bottom_fade()), (1.0, 1.0));
        assert_eq!((at(188.0).top_fade(), at(188.0).bottom_fade()), (1.0, 0.5));
        assert_eq!((at(200.0).top_fade(), at(200.0).bottom_fade()), (1.0, 0.0));
    }

    #[test]
    fn content_scrolls_under_window() {
        let middle = at(100.0);
        assert_eq!(middle.spare(), 200.0);
        assert_eq!(middle.content(), Rect::from_min_size(pos2(16.0, -20.0), vec2(400.0, 600.0)));
        assert!(middle.clip().contains_rect(WINDOW));
        for offset in [0.0, 100.0, 200.0] {
            let bar = at(offset).scrollbar();
            assert!(at(offset).clip().contains_rect(bar), "the scrollbar is never clipped away: {bar:?}");
            assert!(bar.left() > WINDOW.right(), "it sits in the gutter, beside the content");
        }
        assert_eq!(at(0.0).scrollbar().top(), WINDOW.top());
        assert_eq!(at(200.0).scrollbar().bottom(), WINDOW.bottom());
    }

    #[test]
    fn fades_solid_at_edge() {
        let body = Color32::from_black_alpha(140);
        let top = stops(1.0, true, body);
        assert_eq!((top[0], top[FADE_STOPS]), (body, Color32::TRANSPARENT), "it fades into the panel's own color");
        let bottom = stops(0.5, false, Color32::BLACK);
        assert_eq!((bottom[0], bottom[FADE_STOPS]), (Color32::TRANSPARENT, Color32::BLACK.gamma_multiply(0.5)));
        assert!(stops(0.0, false, body).iter().all(|stop| *stop == Color32::TRANSPARENT));
    }

    #[test]
    fn fades_then_scrollbar() {
        let painter = Painter::new(egui::Context::default(), egui::LayerId::background(), Rect::EVERYTHING);
        let mut canvas = Canvas::new(&painter);
        at(100.0).paint(&mut canvas, [Color32::BLACK; 2]);
        let meshes: Vec<bool> = canvas.shapes.iter().map(|shape| matches!(shape, Shape::Mesh(_))).collect();
        assert_eq!(meshes, [true, true, false]);
        let Shape::Mesh(bottom) = &canvas.shapes[1] else { unreachable!() };
        assert_eq!(bottom.calc_bounds(), Rect::from_x_y_ranges(WINDOW.x_range(), 456.0..=480.0));
    }

    #[test]
    fn clear_glass_dims_rows() {
        let painter = Painter::new(egui::Context::default(), egui::LayerId::background(), Rect::EVERYTHING);
        let mut canvas = Canvas::new(&painter);
        let row = |y: f32| Rect::from_center_size(pos2(200.0, y), vec2(300.0, 20.0));
        for y in [476.0, 300.0] {
            canvas.rect(row(y), 0.0, Color32::WHITE);
        }
        at(100.0).paint(&mut canvas, [Color32::BLACK, Color32::BLACK.gamma_multiply(0.08)]);
        let fill = |shape: &Shape| match shape {
            Shape::Rect(rect) => rect.fill.a(),
            _ => 0,
        };
        assert!(fill(&canvas.shapes[0]) < 80, "a row at the bottom edge is faded instead of painted over");
        assert_eq!(fill(&canvas.shapes[1]), 255, "one in the middle is untouched");
        let meshes = canvas.shapes.iter().filter(|shape| matches!(shape, Shape::Mesh(_))).count();
        assert_eq!(meshes, 1, "only the solid top edge gets a band");
    }
}
