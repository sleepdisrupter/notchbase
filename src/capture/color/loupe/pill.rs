//! The pill under the lens with a swatch and the hex value. Near the bottom of the screen it goes above the lens,
//! and after a pick the swatch turns into a check.

use egui::{Align2, Color32, FontId, Pos2, Rect, Vec2, pos2, vec2};

use super::lens::BEZEL_RADIUS;
use crate::capture::overlay::paint::{EDGE_MARGIN, label_shadow, snap_pos};
use crate::ui::canvas::Canvas;
use crate::ui::color;
use crate::ui::glass::{self, Glass};
use crate::ui::glyph::{self, Glyph};

const GAP: f32 = 10.0;
const HEIGHT: f32 = 26.0;
const PAD_LEFT: f32 = HEIGHT * 0.5 - SWATCH_RADIUS;
const PAD_RIGHT: f32 = 11.0;
const SWATCH_RADIUS: f32 = 6.0;
const SWATCH_GAP: f32 = 7.0;

pub(super) fn size(text_width: f32) -> Vec2 {
    vec2(PAD_LEFT + 2.0 * SWATCH_RADIUS + SWATCH_GAP + text_width + PAD_RIGHT, HEIGHT)
}

pub(super) fn overflows_below(center: Pos2, size: Vec2, bounds: Rect) -> bool {
    center.y + BEZEL_RADIUS + GAP + size.y > bounds.bottom() - EDGE_MARGIN
}

pub(super) fn rect(center: Pos2, size: Vec2, bounds: Rect, above: bool, ppp: f32) -> Rect {
    let min_x = bounds.left() + EDGE_MARGIN;
    let max_x = (bounds.right() - EDGE_MARGIN - size.x).max(min_x);
    let x = (center.x - size.x * 0.5).clamp(min_x, max_x);
    let y = if above { center.y - BEZEL_RADIUS - GAP - size.y } else { center.y + BEZEL_RADIUS + GAP };
    Rect::from_min_size(snap_pos(pos2(x, y), ppp), size)
}

pub(super) struct Label<'a> {
    pub(super) glass: Glass,
    pub(super) swatch: Color32,
    pub(super) hex: &'a str,
    pub(super) font: FontId,
    pub(super) check: f32,
}

pub(super) fn paint(canvas: &mut Canvas, rect: Rect, label: &Label) {
    label_shadow(canvas, rect);
    glass::paint(canvas, rect, rect.height() * 0.5, label.glass);
    let dot = pos2(rect.left() + PAD_LEFT + SWATCH_RADIUS, rect.center().y);
    canvas.circle(dot, SWATCH_RADIUS + 0.5, label.swatch);
    glass::ring(canvas, dot, SWATCH_RADIUS, SWATCH_RADIUS + 1.5, Glass::CONTROL);
    if label.check > 0.0 {
        canvas.circle(dot, SWATCH_RADIUS + 1.5, color::BLUE.gamma_multiply(label.check));
        glyph::paint(canvas, Glyph::Checkmark, dot, 9.0, color::white(label.check));
    }
    let text = pos2(dot.x + SWATCH_RADIUS + SWATCH_GAP, rect.center().y);
    canvas.text(text, Align2::LEFT_CENTER, label.hex, label.font.clone(), color::label());
}

#[cfg(test)]
mod tests {
    use super::super::placement::{CENTER, WINDOW};
    use super::*;

    #[test]
    fn window_fits_loupe_and_label() {
        let window = Rect::from_min_size(Pos2::ZERO, WINDOW);
        let size = size(7.0 * 8.0);
        assert!(!overflows_below(CENTER, size, window));
        for above in [false, true] {
            let pill = rect(CENTER, size, window, above, 1.0);
            assert!(window.contains_rect(pill), "{pill:?}");
        }
        const { assert!(CENTER.x >= BEZEL_RADIUS + 20.0 && CENTER.y >= BEZEL_RADIUS + 20.0) };
        let bounds = Rect::from_min_max(pos2(CENTER.x, 0.0), WINDOW.to_pos2());
        let pill = rect(CENTER, size, bounds, false, 1.0);
        assert!(pill.left() >= CENTER.x + EDGE_MARGIN - 0.5 && window.contains_rect(pill), "{pill:?}");
    }

    #[test]
    fn label_flips_above_near_bottom() {
        let size = vec2(90.0, HEIGHT);
        let bounds = Rect::from_min_max(pos2(0.0, 0.0), pos2(240.0, 150.0));
        assert!(overflows_below(CENTER, size, bounds));
        let pill = rect(CENTER, size, bounds, true, 1.5);
        assert!(pill.bottom() <= CENTER.y - BEZEL_RADIUS - GAP + 0.5);
    }
}
