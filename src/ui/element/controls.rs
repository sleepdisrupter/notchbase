//! Paints the nodes you can press or drag, like buttons, toggles, chips, dropdowns and sliders, and reports changes.

use egui::{Align2, Color32, Id, Rect, Sense, Vec2, pos2};
use notchbase_sdk::protocol::{ButtonStyle, Node, UiValue};

use super::Scene;
use super::measure::{BUTTON_ICON, BUTTON_ICON_GAP, CHECK, CHECK_GAP, CHIP_GLYPH, CHIP_TEXT_GAP, dropdown_title};
use super::style::{accent, glyph};
use crate::ui::canvas::{self, Canvas};
use crate::ui::color::{self, label, secondary_label, tertiary_label};
use crate::ui::controls::{self, Slider};
use crate::ui::font;
use crate::ui::glass::{self, Glass};
use crate::ui::glyph::{self as glyphs, Glyph};
use crate::ui::segmented::segmented;
use crate::ui::switch::switch;

const DISABLED: f32 = 0.45;
const CHECK_RADIUS: f32 = 4.0;
const CHECK_GLYPH: f32 = 11.0;

pub(super) fn handles(node: &Node) -> bool {
    matches!(
        node,
        Node::Button { .. }
            | Node::Toggle { .. }
            | Node::Checkbox { .. }
            | Node::Chip { .. }
            | Node::Dropdown { .. }
            | Node::Segmented { .. }
            | Node::Slider { .. }
            | Node::Picker { .. }
            | Node::RoundButton { .. }
    )
}

pub(super) fn paint(scene: &mut Scene, node: &Node, rect: Rect, id: Id) {
    match node {
        Node::Button { label, icon, style, on_press } => {
            let enabled = on_press.is_some();
            let mark = scene.canvas.mark();
            let pressed = button(scene, rect, id, label, icon.map(glyph), *style, enabled);
            if !enabled {
                fade_since(scene.canvas, mark, DISABLED);
            }
            if pressed {
                scene.emit(*on_press, UiValue::Press);
            }
        }
        Node::Toggle { on, on_toggle } => {
            let live = scene.live && on_toggle.is_some();
            if switch(scene.ui, scene.canvas, rect, id, *on, live) {
                crate::platform::haptic(crate::platform::Feel::Flip);
                scene.emit(*on_toggle, UiValue::Bool(!on));
            }
        }
        Node::Checkbox { label, checked, on_toggle } => {
            if checkbox(scene, rect, id, label, *checked, on_toggle.is_some()) {
                scene.emit(*on_toggle, UiValue::Bool(!checked));
            }
        }
        Node::Chip { label, icon, icon_off, on, on_toggle } => {
            let glyph = if *on { *icon } else { icon_off.or(*icon) };
            if chip(scene, rect, id, label, glyph.map(super::style::glyph), *on, on_toggle.is_some()) {
                scene.emit(*on_toggle, UiValue::Bool(!on));
            }
        }
        Node::Dropdown { options, selected, placeholder, on_select, compact } => {
            let open = scene.state.drawer.is_open_for(id);
            let clicked = if *compact {
                let shown = options.get(selected.unwrap_or(0)).and_then(|choice| choice.icon);
                let live = scene.live && on_select.is_some();
                chip(scene, rect, id, "", shown.map(glyph), selected.is_some() || open, live)
            } else {
                let title = dropdown_title(options, *selected, placeholder);
                controls::popup_button(scene.ui, scene.canvas, rect, id, title, open).clicked()
            };
            if clicked && scene.live && !options.is_empty() && on_select.is_some() {
                scene.state.drawer.toggle(id, options.len(), *selected);
            }
            scene.state.drawer.anchor(id, rect, options, *selected, *on_select);
        }
        Node::Segmented { options, selected, on_select } => {
            let titles: Vec<&str> = options.iter().map(String::as_str).collect();
            let live = scene.live && on_select.is_some();
            if let Some(index) = segmented(scene.ui, scene.canvas, rect, id, &titles, *selected, live) {
                scene.emit(*on_select, UiValue::Index(index));
            }
        }
        Node::Slider { value, min, max, step, icon, on_change } => {
            let span = (max - min).max(f32::EPSILON);
            let slider = Slider {
                value: ((value - min) / span).clamp(0.0, 1.0),
                level: None,
                leading: icon.map(glyph),
                dimmed: 0.0,
                enabled: scene.live && on_change.is_some(),
                thickness: None,
            };
            if let Some(changed) = slider.show(scene.ui, scene.canvas, rect, id).changed {
                let mut next = min + changed * span;
                if let Some(step) = step.filter(|step| *step > 0.0) {
                    next = (min + ((next - min) / step).round() * step).clamp(*min, *max);
                }
                if (next - value).abs() > f32::EPSILON {
                    scene.emit(*on_change, UiValue::Number(next));
                }
            }
        }
        Node::Picker { columns, on_change } => super::picker::paint(scene, rect, id, columns, *on_change),
        Node::RoundButton { label, tone, size, on_press } => {
            let round = super::round::Round { label, tone: *tone, size: *size, enabled: on_press.is_some() };
            if super::round::paint(scene, rect, id, &round) {
                scene.emit(*on_press, UiValue::Press);
            }
        }
        _ => {}
    }
}

fn button(
    scene: &mut Scene,
    rect: Rect,
    id: Id,
    text: &str,
    icon: Option<Glyph>,
    style: ButtonStyle,
    enabled: bool,
) -> bool {
    let live = scene.live && enabled;
    let response = scene.ui.interact(rect, id, if live { Sense::click() } else { Sense::hover() });
    let hover = controls::animate(scene.ui, id.with("hover"), live && response.hovered());
    let radius = rect.height() * 0.5;
    let text_color = match style {
        ButtonStyle::Glass => {
            glass::paint(scene.canvas, rect, radius, Glass::lerp(Glass::CONTROL, Glass::TILE_HOVER, hover));
            label()
        }
        ButtonStyle::Filled => {
            let tint = Glass::tinted_by(accent());
            glass::paint(
                scene.canvas,
                rect,
                radius,
                Glass::lerp(tint, Glass::tinted_by(color::mix(accent(), Color32::WHITE, 0.2)), hover),
            );
            Color32::WHITE
        }
        ButtonStyle::Destructive => {
            let tint = Glass::tinted_by(color::RED);
            glass::paint(
                scene.canvas,
                rect,
                radius,
                Glass::lerp(tint, Glass::tinted_by(color::mix(color::RED, Color32::WHITE, 0.2)), hover),
            );
            Color32::WHITE
        }
        ButtonStyle::Plain => {
            controls::row_highlight(scene.canvas, rect, hover);
            accent()
        }
        ButtonStyle::Tinted(tone) => {
            let tone = super::style::ink(tone);
            let tint = Glass::tinted_by(tone);
            glass::paint(
                scene.canvas,
                rect,
                radius,
                Glass::lerp(tint, Glass::tinted_by(color::mix(tone, Color32::WHITE, 0.2)), hover),
            );
            Color32::WHITE
        }
    };
    let text_width = scene.canvas.text_width(text, font::callout());
    let gap = if text.is_empty() { 0.0 } else { BUTTON_ICON_GAP };
    let icon_width = if icon.is_some() { BUTTON_ICON + gap } else { 0.0 };
    let left = rect.center().x - (text_width + icon_width) * 0.5;
    if let Some(icon) = icon {
        glyphs::paint(scene.canvas, icon, pos2(left + BUTTON_ICON * 0.5, rect.center().y), BUTTON_ICON, text_color);
    }
    let room = rect.width() - icon_width - 8.0;
    let shown = controls::truncate(scene.canvas, text, font::callout(), room);
    let at = pos2(left + icon_width, rect.center().y);
    scene.canvas.text(at, Align2::LEFT_CENTER, shown, font::callout(), text_color);
    live && response.clicked()
}

fn checkbox(scene: &mut Scene, rect: Rect, id: Id, text: &str, checked: bool, enabled: bool) -> bool {
    let live = scene.live && enabled;
    let response = scene.ui.interact(rect, id, if live { Sense::click() } else { Sense::hover() });
    let hover = controls::animate(scene.ui, id.with("hover"), live && response.hovered());
    let lit = controls::animate(scene.ui, id.with("on"), checked);
    let mark = scene.canvas.mark();
    let square = Rect::from_center_size(pos2(rect.left() + CHECK * 0.5, rect.center().y), Vec2::splat(CHECK));
    let resting = Glass::lerp(Glass::CONTROL, Glass::TILE_HOVER, hover * 0.6);
    glass::paint(scene.canvas, square, CHECK_RADIUS, resting);
    if lit > 0.001 {
        scene.canvas.rect(square, CHECK_RADIUS, color::ACCENT.gamma_multiply(lit));
        glyphs::paint(
            scene.canvas,
            Glyph::Checkmark,
            square.center(),
            CHECK_GLYPH,
            color::ink_on(color::ACCENT).gamma_multiply(lit),
        );
    }
    let left = square.right() + CHECK_GAP;
    let shown = controls::truncate(scene.canvas, text, font::body(), rect.right() - left);
    scene.canvas.text(pos2(left, rect.center().y), Align2::LEFT_CENTER, shown, font::body(), label());
    if !enabled {
        fade_since(scene.canvas, mark, DISABLED);
    }
    live && response.clicked()
}

fn chip(scene: &mut Scene, rect: Rect, id: Id, text: &str, icon: Option<Glyph>, on: bool, enabled: bool) -> bool {
    let live = scene.live && enabled;
    let response = scene.ui.interact(rect, id, if live { Sense::click() } else { Sense::hover() });
    let hover = controls::animate(scene.ui, id.with("hover"), live && response.hovered());
    let lit = controls::animate(scene.ui, id.with("on"), on);
    let resting = Glass::lerp(Glass::CONTROL, Glass::TILE_HOVER, hover * 0.6);
    glass::paint(scene.canvas, rect, rect.height() * 0.5, Glass::lerp(resting, Glass::tinted_by(color::ACCENT), lit));
    let text_width = scene.canvas.text_width(text, font::section());
    let glyph_width = match icon {
        Some(_) if text.is_empty() => CHIP_GLYPH,
        Some(_) => CHIP_GLYPH + CHIP_TEXT_GAP,
        None => 0.0,
    };
    let left = rect.center().x - (glyph_width + text_width) * 0.5;
    if let Some(icon) = icon {
        let glyph_color = color::mix(tertiary_label(), color::ACCENT, lit);
        glyphs::paint(scene.canvas, icon, pos2(left + CHIP_GLYPH * 0.5, rect.center().y), CHIP_GLYPH, glyph_color);
    }
    let tint = color::mix(secondary_label(), label(), lit);
    scene.canvas.text(pos2(left + glyph_width, rect.center().y), Align2::LEFT_CENTER, text, font::section(), tint);
    live && response.clicked()
}

fn fade_since(canvas: &mut Canvas, mark: canvas::Mark, alpha: f32) {
    for shape in canvas.since(mark) {
        canvas::fade(shape, alpha);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_interactive_nodes_are_controls() {
        let toggle = Node::Toggle { on: true, on_toggle: None };
        let button = Node::Button { label: "Go".into(), icon: None, style: ButtonStyle::default(), on_press: None };
        let checkbox = Node::Checkbox { label: "Keep".into(), checked: false, on_toggle: None };
        assert!(handles(&toggle) && handles(&button) && handles(&checkbox));
        assert!(!handles(&Node::Divider));
        assert!(!handles(&Node::Spacer { size: None }));
        assert!(!handles(&Node::Column { children: vec![toggle], spacing: None, align: Default::default() }));
    }
}
