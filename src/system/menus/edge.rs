//! The right edge of the menus on the notch's screen, measured from the canvas's left side.

use egui::{Pos2, Rect};

const SAME_BAR: f32 = 4.0;

pub fn edge_in(items: &[Rect], origin: Pos2, canvas_width: f32) -> Option<f32> {
    let middle = origin.x + canvas_width * 0.5;
    items
        .iter()
        .filter(|item| (item.top() - origin.y).abs() <= SAME_BAR && item.right() > origin.x && item.center().x < middle)
        .map(|item| item.right() - origin.x)
        .reduce(f32::max)
}

#[cfg(test)]
mod tests {
    use egui::{pos2, vec2};

    use super::*;

    fn item(left: f32, top: f32, width: f32) -> Rect {
        Rect::from_min_size(pos2(left, top), vec2(width, 24.0))
    }

    #[test]
    fn rightmost_menu_sets_the_edge() {
        let origin = pos2(416.0, 0.0);
        let items = [item(10.0, 0.0, 30.0), item(600.0, 0.0, 44.0), item(660.0, 0.0, 40.0)];
        assert_eq!(edge_in(&items, origin, 680.0), Some(284.0));
    }

    #[test]
    fn far_menus_do_not_count() {
        let origin = pos2(1512.0, 0.0);
        let elsewhere = [item(10.0, 0.0, 30.0), item(1600.0, -1080.0, 40.0)];
        assert_eq!(edge_in(&elsewhere, origin, 680.0), None);
        let right_half = [item(1512.0 + 400.0, 0.0, 40.0)];
        assert_eq!(edge_in(&right_half, origin, 680.0), None);
    }
}
