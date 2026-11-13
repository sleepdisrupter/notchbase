//! The size label that follows the selection: below it, above it or inside it, whichever fits, gliding as it moves.

use egui::{Align2, Color32, Rect, Vec2, pos2, vec2};

use super::geometry::PixelRect;
use crate::capture::overlay::glide::Glide;
use crate::capture::overlay::paint::{EDGE_MARGIN, label_shadow, snap_pos};
use crate::ui::canvas::Canvas;
use crate::ui::glass::{self, Glass};
use crate::ui::{color, font, motion};

const HEIGHT: f32 = 22.0;
const PAD: f32 = 10.0;
const GAP: f32 = 8.0;

pub(super) struct SizeLabel {
    x: Glide,
    y: Glide,
}

impl SizeLabel {
    pub(super) fn new() -> Self {
        Self { x: Glide::new(motion::HOVER, 0.05), y: Glide::new(motion::HOVER, 0.05) }
    }

    pub(super) fn reset(&mut self) {
        self.x.reset();
        self.y.reset();
    }

    pub(super) fn is_animating(&self) -> bool {
        !self.x.is_settled() || !self.y.is_settled()
    }

    pub(super) fn paint(&mut self, canvas: &mut Canvas, pixels: PixelRect, selection: Rect, view: Vec2, dt: f32) {
        let ppp = canvas.pixels_per_point();
        let text = format!("{} \u{d7} {}", pixels.width(), pixels.height());
        let font = font::value();
        let width = canvas.text_width(text.as_str(), font.clone()) + 2.0 * PAD;
        let size = vec2(width.round(), HEIGHT);
        let target = placement(selection, size, view).min;
        let at = pos2(self.x.follow(target.x, dt), self.y.follow(target.y, dt));
        let label = Rect::from_min_size(snap_pos(at, ppp), size);
        label_shadow(canvas, label);
        glass::paint(canvas, label, label.height() * 0.5, Glass::tinted_by(Color32::from_gray(48)));
        canvas.text(label.center(), Align2::CENTER_CENTER, text, font, color::label());
    }
}

fn placement(selection: Rect, size: Vec2, view: Vec2) -> Rect {
    let below = selection.bottom() + GAP;
    let above = selection.top() - GAP - size.y;
    let y = if below + size.y <= view.y - EDGE_MARGIN {
        below
    } else if above >= EDGE_MARGIN {
        above
    } else {
        (selection.bottom() - GAP - size.y).clamp(EDGE_MARGIN, (view.y - EDGE_MARGIN - size.y).max(EDGE_MARGIN))
    };
    Rect::from_min_size(pos2(centered_x(selection.center().x, size.x, view), y), size)
}

fn centered_x(x: f32, width: f32, view: Vec2) -> f32 {
    (x - width * 0.5).clamp(EDGE_MARGIN, (view.x - EDGE_MARGIN - width).max(EDGE_MARGIN))
}

#[cfg(test)]
mod tests {
    use egui::Pos2;

    use super::*;

    #[test]
    fn label_goes_below_above_then_inside() {
        let view = vec2(1440.0, 900.0);
        let size = vec2(100.0, HEIGHT);
        let middle = Rect::from_min_max(pos2(200.0, 200.0), pos2(600.0, 500.0));
        assert!((placement(middle, size, view).top() - (500.0 + GAP)).abs() < 1e-3);
        let low = Rect::from_min_max(pos2(200.0, 400.0), pos2(600.0, 890.0));
        assert!((placement(low, size, view).bottom() - (400.0 - GAP)).abs() < 1e-3);
        let full = Rect::from_min_size(Pos2::ZERO, view);
        assert!(full.contains_rect(placement(full, size, view)));
        let edge = Rect::from_min_max(pos2(0.0, 100.0), pos2(20.0, 200.0));
        assert!(placement(edge, size, view).left() >= EDGE_MARGIN);
    }
}
