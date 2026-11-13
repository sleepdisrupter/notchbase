//! A segmented control with a glass pill that slides to the chosen segment.

use egui::{Align2, Id, Rect, Sense, pos2, vec2};

use super::canvas::Canvas;
use super::color::{self, label, secondary_label};
use super::controls;
use super::font;
use super::glass::{self, Glass};
use super::spring::smoothstep;

const SLIDE_SECONDS: f32 = 0.22;
const INSET: f32 = 2.0;

pub fn segmented(
    ui: &egui::Ui,
    canvas: &mut Canvas,
    rect: Rect,
    id: Id,
    titles: &[&str],
    selected: usize,
    live: bool,
) -> Option<usize> {
    if titles.is_empty() {
        return None;
    }
    let width = rect.width() / titles.len() as f32;
    let segment = |index: usize| {
        Rect::from_min_size(pos2(rect.left() + width * index as f32, rect.top()), vec2(width, rect.height()))
    };
    canvas.rect(rect, rect.height() * 0.5, color::quaternary_label());
    let slid = ui.ctx().animate_value_with_time(id.with("at"), selected as f32, SLIDE_SECONDS);
    let at = slid.floor() + smoothstep(0.0, 1.0, slid.fract());
    let pill =
        Rect::from_min_size(pos2(rect.left() + width * at, rect.top()), vec2(width, rect.height())).shrink(INSET);
    if selected < titles.len() {
        glass::paint(canvas, pill, pill.height() * 0.5, Glass::CONTROL);
    }

    let mut picked = None;
    for (index, title) in titles.iter().enumerate() {
        let slot = segment(index);
        let hovered = live && {
            let response = ui.interact(slot, id.with(index), Sense::click());
            if response.clicked() && index != selected {
                picked = Some(index);
            }
            response.hovered()
        };
        let hover = controls::animate(ui, id.with(("hover", index)), hovered);
        let emphasis = (1.0 - (at - index as f32).abs()).clamp(0.0, 1.0).max(0.5 * hover);
        let tint = color::mix(secondary_label(), label(), emphasis);
        canvas.text(slot.center(), Align2::CENTER_CENTER, *title, font::section(), tint);
    }
    picked
}
