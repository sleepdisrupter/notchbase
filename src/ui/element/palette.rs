//! A row of color swatches with a ring that glides to the chosen one.

use egui::{Color32, Id, Rect, Sense, Stroke, pos2, vec2};
use notchbase_sdk::protocol::{Color, HandlerId, UiValue};

use super::Scene;
use super::liquid;
use super::style::ink;
use crate::ui::controls;

pub const SLOT: f32 = 28.0;
const RADIUS: f32 = 9.0;
const GROW: f32 = 1.5;
const RING: f32 = 13.0;
const RING_WIDTH: f32 = 2.0;

pub fn size(count: usize) -> egui::Vec2 {
    vec2(count as f32 * SLOT, SLOT)
}

pub(super) fn paint(
    scene: &mut Scene,
    colors: &[Color],
    selected: Option<usize>,
    on_select: Option<HandlerId>,
    rect: Rect,
    id: Id,
) {
    let slot =
        |index: usize| Rect::from_min_size(pos2(rect.left() + index as f32 * SLOT, rect.top()), vec2(SLOT, SLOT));
    if let Some(chosen) = selected.filter(|index| *index < colors.len()) {
        let ctx = scene.ui.ctx().clone();
        let (_, lead) = liquid::follow(&ctx, id.with("ring"), slot(chosen).center());
        scene.canvas.ring(lead, RING, Stroke::new(RING_WIDTH, Color32::WHITE));
    }
    let live = scene.live && on_select.is_some();
    for (index, color) in colors.iter().enumerate() {
        let cell = slot(index);
        let swatch_id = id.with(("color", index));
        let response = scene.ui.interact(cell, swatch_id, if live { Sense::click() } else { Sense::hover() });
        let hover = controls::animate(scene.ui, swatch_id.with("hover"), live && response.hovered());
        scene.canvas.circle(cell.center(), RADIUS + GROW * hover, ink(*color));
        if live && response.clicked() {
            scene.emit(on_select, UiValue::Index(index));
        }
    }
}
