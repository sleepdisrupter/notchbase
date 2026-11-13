//! The slim row a running tool adds under the icons.

use egui::{Align2, Id, Rect, Sense, Vec2, pos2};
use notchbase_sdk::protocol::{TileNode, Tone};

use super::tile::TileState;
use crate::ui::canvas::scale_about;
use crate::ui::color::label;
use crate::ui::element::Scene;
use crate::ui::element::style::tone;
use crate::ui::glass::{self, Glass};
use crate::ui::{controls, font};

pub(super) const ROW_H: f32 = 34.0;
const INSET: f32 = 6.0;
const WELL: f32 = 22.0;
const TITLE_GAP: f32 = 9.0;
const FOOTER_GAP: f32 = 8.0;
const TINT: f32 = 0.55;

pub(super) fn is_running(tile: &TileNode) -> bool {
    tile.tint.is_some()
}

pub(super) fn paint(scene: &mut Scene, tile: &TileNode, rect: Rect, state: &mut TileState, pressable: bool) -> bool {
    let id = Id::new(("notchbase.running", scene.slot.0));
    let pressable = pressable && scene.live && tile.enabled;
    let body = scene.ui.interact(rect, id, if pressable { Sense::click() } else { Sense::hover() });
    let mark = scene.canvas.mark();
    let hover = state.hover().clamp(0.0, 1.0);
    let base = Glass::lerp(Glass::TILE, Glass::TILE_HOVER, hover);
    let surface = Glass::lerp(base, Glass::tinted_by(tone(tile.tint.unwrap_or(Tone::Gray))), TINT);
    glass::paint(scene.canvas, rect, rect.height() * 0.5, surface);
    let well = pos2(rect.left() + INSET + WELL * 0.5, rect.center().y);
    match tile.well {
        Some(t) if crate::ui::color::is_light(tone(t)) => {
            let well = egui::Rect::from_center_size(well, egui::Vec2::splat(WELL));
            glass::paint(scene.canvas, well, WELL * 0.5, Glass::tinted_by(tone(t)));
        }
        Some(t) => scene.canvas.circle(well, WELL * 0.5, tone(t)),
        None => glass::paint(scene.canvas, Rect::from_center_size(well, Vec2::splat(WELL)), WELL * 0.5, Glass::CONTROL),
    }
    let icon = scene.measure().size(&tile.icon, WELL);
    scene.paint(&tile.icon, Rect::from_center_size(well, icon), id.with("icon"));
    let right = rect.right() - rect.height() * 0.5;
    let title_left = well.x + WELL * 0.5 + TITLE_GAP;
    let footer = tile.footer.as_ref().map(|node| {
        let size = scene.measure().size(node, (right - title_left).max(0.0));
        (node, Rect::from_min_size(pos2(right - size.x, rect.center().y - size.y * 0.5), size))
    });
    let title_right = footer.map_or(right, |(_, area)| area.left() - FOOTER_GAP);
    let title = controls::truncate(scene.canvas, &tile.title, font::callout(), (title_right - title_left).max(0.0));
    scene.canvas.text(pos2(title_left, rect.center().y), Align2::LEFT_CENTER, title, font::callout(), label());
    if let Some((node, area)) = footer {
        scene.paint(node, area, id.with("footer"));
    }
    let live = scene.live;
    state.follow(live && body.hovered(), pressable && body.is_pointer_button_down_on());
    let press = scale_about(rect.center(), state.pressed());
    for shape in scene.canvas.since(mark) {
        shape.transform(press);
    }
    pressable && body.clicked()
}
