//! One icon's animated state: hover, press, and the tint of a tool that is live.

use egui::{Align2, Id, Rect, Sense, Vec2, pos2, vec2};
use notchbase_sdk::protocol::{TileNode, Tone, UiValue};

use crate::bridge::Action;
use crate::ui::canvas::{Mark, scale_about};
use crate::ui::color::label;
use crate::ui::element::Scene;
use crate::ui::element::shortcut::{KEYCAP_H, KEYS_SLOP};
use crate::ui::element::style::tone;
use crate::ui::glass::{self, Glass};
use crate::ui::metrics::CARD_RADIUS;
use crate::ui::spring::{Spring, UNIT_PRECISION};
use crate::ui::{controls, font, motion};

use super::shortcut;

pub(super) const TILE_H: f32 = 76.0;
const INSET: f32 = 12.0;
const ICON: f32 = 28.0;
const TITLE_GAP: f32 = 9.0;
const TITLE_TRAILING: f32 = 10.0;
const ACCESSORY_GAP: f32 = 8.0;
const FOOTER_GAP: f32 = 8.0;
const FIELD_H: f32 = 20.0;
const PRESSED: f32 = 0.96;
const LIVE_TINT: f32 = 0.5;

pub(super) struct TileState {
    hover: Spring,
    press: Spring,
    tint: Spring,
    tone: Option<Tone>,
}

impl Default for TileState {
    fn default() -> Self {
        let unit = |value: f32| Spring::new(value, motion::HOVER, UNIT_PRECISION);
        Self { hover: unit(0.0), press: unit(1.0), tint: unit(0.0), tone: None }
    }
}

impl TileState {
    pub(super) fn follow(&mut self, hovered: bool, down: bool) {
        self.hover.animate_to(if hovered { 1.0 } else { 0.0 }, motion::HOVER);
        self.press.animate_to(if down { PRESSED } else { 1.0 }, motion::PRESS);
    }

    pub(super) fn springs_mut(&mut self) -> [&mut Spring; 3] {
        [&mut self.hover, &mut self.press, &mut self.tint]
    }

    pub(super) fn springs(&self) -> [&Spring; 3] {
        [&self.hover, &self.press, &self.tint]
    }

    pub(super) fn tint(&mut self, tint: Option<Tone>) -> Option<(Tone, f32)> {
        if tint.is_some() {
            self.tone = tint;
        }
        self.tint.animate_to(if tint.is_some() { 1.0 } else { 0.0 }, motion::HOVER);
        self.tone.map(|tone| (tone, self.tint.value.clamp(0.0, 1.0))).filter(|(_, shown)| *shown > 0.001)
    }

    pub(super) fn hover(&self) -> f32 {
        self.hover.value
    }

    pub(super) fn pressed(&self) -> f32 {
        self.press.value
    }
}

pub(super) fn paint(
    scene: &mut Scene,
    tile: &TileNode,
    rect: Rect,
    state: &mut TileState,
    (opens, glassed): (bool, bool),
) -> Mark {
    let id = Id::new(("notchbase.tile", scene.slot.0));
    let opens = opens && tile.on_press.is_none();
    let pressable = scene.live && tile.enabled && (tile.on_press.is_some() || opens);
    let body = scene.ui.interact(rect, id, if pressable { Sense::click() } else { Sense::hover() });
    let mark = scene.canvas.mark();
    let hover = state.hover.value.clamp(0.0, 1.0);
    let base = Glass::lerp(Glass::TILE, Glass::TILE_HOVER, hover);
    let tint = state.tint(tile.tint);
    let surface = tint.map_or(base, |(t, shown)| Glass::lerp(base, Glass::tinted_by(tone(t)), LIVE_TINT * shown));
    if glassed {
        glass::paint(scene.canvas, rect, CARD_RADIUS, surface);
    }
    let contents = scene.canvas.mark();

    let well = rect.min + Vec2::splat(INSET + ICON * 0.5);
    match tile.well {
        Some(t) if crate::ui::color::is_light(tone(t)) => {
            let well = egui::Rect::from_center_size(well, egui::Vec2::splat(ICON));
            glass::paint(scene.canvas, well, ICON * 0.5, Glass::tinted_by(tone(t)));
        }
        Some(t) => scene.canvas.circle(well, ICON * 0.5, tone(t)),
        None => glass::paint(scene.canvas, Rect::from_center_size(well, Vec2::splat(ICON)), ICON * 0.5, Glass::CONTROL),
    }
    let icon = scene.measure().size(&tile.icon, ICON);
    scene.paint(&tile.icon, Rect::from_center_size(well, icon), id.with("icon"));

    let right = rect.right() - INSET;
    let title_left = well.x + ICON * 0.5 + TITLE_GAP;
    let accessory = tile.accessory.as_ref().map(|node| {
        let size = scene.measure().size(node, (right - title_left).max(0.0));
        (node, Rect::from_min_size(pos2(right - size.x, well.y - size.y * 0.5), size))
    });
    let title_right = accessory.map_or(rect.right() - TITLE_TRAILING, |(_, area)| area.left() - ACCESSORY_GAP);
    let title = controls::truncate(scene.canvas, &tile.title, font::headline(), (title_right - title_left).max(0.0));
    scene.canvas.text(pos2(title_left, well.y), Align2::LEFT_CENTER, title, font::headline(), label());
    if let Some((node, area)) = accessory {
        scene.paint(node, area, id.with("accessory"));
    }

    let keys_y = rect.bottom() - INSET - KEYCAP_H * 0.5;
    let first_key = scene.keys.len();
    if let Some(name) = &tile.shortcut {
        let field = Rect::from_min_size(
            pos2(rect.left() + INSET, keys_y - FIELD_H * 0.5),
            vec2(rect.width() - 2.0 * INSET, FIELD_H),
        );
        shortcut::paint(scene, name, field, hover);
    }
    let keys = scene.keys.get(first_key).copied();
    if let Some(footer) = &tile.footer {
        let left = keys.map_or(rect.left() + INSET, |area| area.right() + FOOTER_GAP);
        let size = scene.measure().size(footer, (right - left).max(0.0));
        scene.paint(footer, Rect::from_min_size(pos2(right - size.x, keys_y - size.y * 0.5), size), id.with("footer"));
    }

    let keys_hovered = keys.is_some_and(|area| scene.ui.rect_contains_pointer(area.expand(KEYS_SLOP)));
    let live = scene.live;
    state.follow(live && (body.hovered() || keys_hovered), live && pressable && body.is_pointer_button_down_on());
    let press = scale_about(rect.center(), state.press.value);
    for shape in scene.canvas.since(mark) {
        shape.transform(press);
    }
    if pressable && body.clicked() {
        match opens {
            true => scene.actions.push(Action::OpenToolPage(scene.slot)),
            false => scene.emit(tile.on_press, UiValue::Press),
        }
    }
    contents
}

#[cfg(test)]
mod tint_tests {
    use super::*;

    #[test]
    fn live_tint_eases_in_and_out() {
        let mut state = TileState::default();
        assert_eq!(state.tint(None), None);
        assert_eq!(state.tint(Some(Tone::Orange)), None, "it starts from nothing");
        for spring in state.springs_mut() {
            spring.step(1.0 / 60.0);
        }
        let (tone, shown) = state.tint(Some(Tone::Orange)).expect("tinting");
        assert!(tone == Tone::Orange && shown > 0.0 && shown < 1.0, "{shown}");
        for _ in 0..60 {
            state.springs_mut().into_iter().for_each(|spring| spring.step(1.0 / 60.0));
        }
        assert_eq!(state.tint(None).map(|(tone, _)| tone), Some(Tone::Orange), "it keeps its colour while it fades");
    }
}
