//! Turns wheel and trackpad scrolling over a month or week into page turns for the tool.

use egui::{Id, Rect, vec2};
use notchbase_sdk::protocol::{HandlerId, UiValue};

use super::Scene;
use crate::ui::scroll;

pub(super) fn turn(scene: &mut Scene, on_scroll: Option<HandlerId>, rect: Rect, id: Id) {
    if !scene.live || on_scroll.is_none() {
        return;
    }
    let step = step(scene.ui, rect, id);
    if step != 0 {
        scene.emit(on_scroll, UiValue::Number(step as f32));
    }
}

fn step(ui: &egui::Ui, rect: Rect, id: Id) -> i32 {
    if !ui.rect_contains_pointer(rect) {
        return 0;
    }
    let wheel = scroll::read(ui);
    if !wheel.smooth {
        let notches = scroll::take(ui, wheel.across());
        return crate::ui::swipe::notch(ui, id, notches);
    }
    let glide = if scroll::page_scrolls(ui) { vec2(wheel.across(), 0.0) } else { vec2(wheel.x, wheel.y) };
    if scroll::take(ui, glide.x.abs() + glide.y.abs()) == 0.0 {
        return 0;
    }
    crate::ui::swipe::either(ui, id, glide)
}
