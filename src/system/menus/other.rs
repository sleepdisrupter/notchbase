//! Other systems report no menus.

use egui::Rect;

pub(super) fn reader() -> impl FnMut() -> Vec<Rect> {
    Vec::new
}
