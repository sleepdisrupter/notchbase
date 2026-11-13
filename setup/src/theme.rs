//! The installer's layout, in points. The window is as tall as its rows need.

pub const WIDTH: f32 = 372.0;
pub const RADIUS: f32 = 34.0;
pub const PAD: f32 = 16.0;
pub const HEADER_H: f32 = 50.0;
pub const TILE_H: f32 = 72.0;
pub const GAP: f32 = 10.0;
pub const SECTION_H: f32 = 26.0;
pub const ROW_H: f32 = 50.0;
pub const INSET: f32 = 14.0;
pub const FOOTER_GAP: f32 = 16.0;
pub const BUTTON_H: f32 = 30.0;
pub const BUTTON_GAP: f32 = 8.0;
pub const BOTTOM: f32 = 18.0;

pub fn section_top() -> f32 {
    HEADER_H + TILE_H + GAP
}

pub fn card_top() -> f32 {
    section_top() + SECTION_H
}

pub fn height(rows: usize) -> f32 {
    card_top() + rows as f32 * ROW_H + FOOTER_GAP + BUTTON_H + BOTTOM
}

pub fn footer_top(rows: usize) -> f32 {
    height(rows) - BOTTOM - BUTTON_H
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_starts_below_the_header() {
        assert_eq!(section_top(), HEADER_H + TILE_H + GAP);
        assert_eq!(card_top(), section_top() + SECTION_H);
        assert_eq!(card_top(), 158.0);
    }

    #[test]
    fn window_grows_by_a_row_each_row() {
        assert_eq!(height(0), card_top() + FOOTER_GAP + BUTTON_H + BOTTOM);
        assert_eq!(height(4) - height(3), ROW_H);
        assert_eq!(height(4), 422.0);
    }

    #[test]
    fn footer_sits_above_the_edge() {
        assert_eq!(footer_top(4) + BUTTON_H + BOTTOM, height(4));
        assert_eq!(footer_top(4), 374.0);
        assert_eq!(footer_top(3) - card_top(), 3.0 * ROW_H + FOOTER_GAP);
    }
}
