//! The round well behind an icon, with its glyph.

use egui::{Color32, Pos2, Rect, Vec2};

use crate::ui::canvas::Canvas;
use crate::ui::glass::{self, Glass};
use crate::ui::glyph::{self, Glyph};

pub(in crate::pages) const WELL: f32 = 48.0;
pub(super) const ICON_GROW: f32 = 1.5;
const TILE_ICON: f32 = 15.0;

pub(in crate::pages) fn paint(canvas: &mut Canvas, center: Pos2, size: f32, hover: f32) {
    let well = Rect::from_center_size(center, Vec2::splat(size));
    glass::paint(canvas, well, size * 0.5, Glass::lerp(Glass::CONTROL, Glass::TILE_HOVER, hover));
}

pub(in crate::pages) fn glyph(canvas: &mut Canvas, glyph: Glyph, center: Pos2, size: f32, tint: Color32) {
    glyph::paint(canvas, glyph, center, glyph_size(size), tint);
}

pub(in crate::pages) fn glyph_size(size: f32) -> f32 {
    TILE_ICON * ICON_GROW * size / WELL
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyph_fills_any_well() {
        assert_eq!(glyph_size(WELL), TILE_ICON * ICON_GROW);
        assert_eq!(glyph_size(WELL * 0.5), TILE_ICON * ICON_GROW * 0.5);
    }
}
