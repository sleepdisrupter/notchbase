//! A node that slides its child sideways by an offset, and reports drags, scrolls and taps back to the tool.

use egui::{Id, Rect, Sense, pos2, vec2};
use notchbase_sdk::protocol::{HandlerId, Node, UiValue};

use super::Scene;
use crate::ui::scroll;

pub(super) const UNBOUNDED: f32 = 100_000.0;

pub(super) struct Pan<'a> {
    pub child: &'a Node,
    pub offset: f32,
    pub on_pan: Option<HandlerId>,
    pub on_tap: Option<HandlerId>,
}

pub(super) fn paint(scene: &mut Scene, pan: &Pan, rect: Rect, id: Id) {
    let size = scene.measure().size(pan.child, UNBOUNDED);
    let left = left(rect, size.x, pan.offset);
    listen(scene, pan, rect, left, id);
    let mark = scene.canvas.mark();
    scene.paint(pan.child, Rect::from_min_size(pos2(left, rect.top()), vec2(size.x, rect.height())), id.with(0));
    scene.canvas.clip(mark, rect);
}

fn left(rect: Rect, width: f32, offset: f32) -> f32 {
    rect.center().x - width * 0.5 + offset
}

fn listen(scene: &mut Scene, pan: &Pan, rect: Rect, left: f32, id: Id) {
    if !scene.live || (pan.on_pan.is_none() && pan.on_tap.is_none()) {
        return;
    }
    let sense = if pan.on_pan.is_some() { Sense::click_and_drag() } else { Sense::click() };
    let response = scene.ui.interact(rect, id, sense);
    let moved = match pan.on_pan {
        Some(_) if response.dragged() => response.drag_delta().x,
        Some(_) if response.hovered() => scroll::across_points(scene.ui),
        _ => 0.0,
    };
    if moved != 0.0 {
        scene.emit(pan.on_pan, UiValue::Number(moved));
    }
    if let Some(at) = response.interact_pointer_pos().filter(|_| response.clicked()) {
        scene.emit(pan.on_tap, UiValue::Number(at.x - left));
    }
}
