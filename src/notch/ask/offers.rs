//! The choices a question offers, one row each with its check box.

use egui::{Align2, Id, Rect, Response, Sense, Stroke, pos2, vec2};

use crate::assistant::Question;
use crate::ui::canvas::Canvas;
use crate::ui::color::{self, label, secondary_label, tertiary_label};
use crate::ui::glass::{self, Glass};
use crate::ui::glyph::{self, Glyph};
use crate::ui::{controls, font};

const ROW_H: f32 = 34.0;
const ROW_GAP: f32 = 6.0;
const RADIUS: f32 = 11.0;
const INSET: f32 = 12.0;
const LABEL_GAP: f32 = 8.0;
const BOX: f32 = 8.0;
const BOX_GAP: f32 = 10.0;
const CHECK: f32 = 9.0;
const OUTLINE: f32 = 1.5;
const PRESS_INSET: f32 = 1.0;

pub(super) fn height(count: usize) -> f32 {
    if count == 0 { 0.0 } else { count as f32 * ROW_H + (count - 1) as f32 * ROW_GAP }
}

pub(super) fn ui(
    ui: &egui::Ui,
    canvas: &mut Canvas,
    area: Rect,
    id: Id,
    question: &Question,
    picked: &[usize],
    live: bool,
) -> Option<usize> {
    let mut chosen = None;
    for (index, offer) in question.offers.iter().enumerate() {
        let top = area.top() + index as f32 * (ROW_H + ROW_GAP);
        let rect = Rect::from_min_size(pos2(area.left(), top), vec2(area.width(), ROW_H));
        let id = id.with(index);
        let response = live.then(|| ui.interact(rect, id, Sense::click()));
        let hover = controls::animate(ui, id.with("hover"), response.as_ref().is_some_and(Response::hovered));
        let down = response.as_ref().is_some_and(Response::is_pointer_button_down_on);
        let on = picked.contains(&index);
        let drawn = if down { rect.shrink(PRESS_INSET) } else { rect };
        glass::paint(canvas, drawn, RADIUS, Glass::lerp(Glass::CONTROL, Glass::TILE_HOVER, hover));
        if on && !question.multi {
            canvas.rect_stroke(drawn, RADIUS, Stroke::new(OUTLINE, color::ACCENT));
        }
        let mut left = rect.left() + INSET;
        if question.multi {
            tick(canvas, pos2(left + BOX, rect.center().y), on);
            left += 2.0 * BOX + BOX_GAP;
        }
        let room = rect.right() - INSET - left;
        let name = controls::truncate(canvas, &offer.label, font::headline(), room);
        let named = canvas.text(pos2(left, rect.center().y), Align2::LEFT_CENTER, name, font::headline(), label());
        let rest = rect.right() - INSET - named.right() - LABEL_GAP;
        if !offer.detail.is_empty() && rest > 24.0 {
            let detail = controls::truncate(canvas, &offer.detail, font::callout(), rest);
            let at = pos2(named.right() + LABEL_GAP, rect.center().y);
            canvas.text(at, Align2::LEFT_CENTER, detail, font::callout(), secondary_label());
        }
        if response.is_some_and(|r| r.clicked()) {
            chosen = Some(index);
        }
    }
    chosen
}

fn tick(canvas: &mut Canvas, center: egui::Pos2, on: bool) {
    if on {
        canvas.circle(center, BOX, color::ACCENT);
        glyph::paint(canvas, Glyph::Checkmark, center, CHECK, color::ink_on(color::ACCENT));
    } else {
        canvas.ring(center, BOX - OUTLINE * 0.5, Stroke::new(OUTLINE, tertiary_label()));
    }
}
