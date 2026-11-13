//! Installer pieces built from the paint kit: cards, badge rows, the close button and the footer.

use egui::{Color32, Id, Pos2, Rect, Response, Sense, pos2, vec2};
use notchbase_paint::buttons;
use notchbase_paint::canvas::Canvas;
use notchbase_paint::color::{self, label, secondary_label};
use notchbase_paint::controls;
use notchbase_paint::glass::{self, Glass};
use notchbase_paint::glyph::{self, Glyph};
use notchbase_paint::list::{self, Lines, TEXT_GAP};

use crate::theme;

const CLOSE: f32 = 13.0;
const CLOSE_GLYPH: f32 = 9.0;
const BADGE: f32 = 14.0;
const BADGE_GLYPH: f32 = 13.0;
const LEAST_BUTTON: f32 = 76.0;

pub fn card(canvas: &mut Canvas, rect: Rect, rows: usize) -> Vec<Rect> {
    let height = rect.height() / rows.max(1) as f32;
    list::card(canvas, rect, &vec![height; rows])
}

pub fn badge_row(canvas: &mut Canvas, rect: Rect, tint: Color32, mark: Glyph, text: &Lines<'_>) {
    let center = pos2(rect.left() + theme::INSET + BADGE, rect.center().y);
    canvas.circle(center, BADGE, tint);
    glyph::paint(canvas, mark, center, BADGE_GLYPH, Color32::WHITE);
    let left = center.x + BADGE + TEXT_GAP;
    list::two_lines(canvas, left, rect.right() - theme::INSET, rect.center().y, text);
}

pub fn close(ui: &egui::Ui, canvas: &mut Canvas, center: Pos2, id: Id, live: bool) -> bool {
    let rect = Rect::from_center_size(center, vec2(CLOSE, CLOSE) * 2.0);
    let response = live.then(|| ui.interact(rect, id, Sense::click()));
    let hover = controls::animate(ui, id.with("hover"), response.as_ref().is_some_and(Response::hovered));
    glass::paint(canvas, rect, CLOSE, Glass::lerp(Glass::CONTROL, Glass::TILE_HOVER, hover));
    glyph::paint(canvas, Glyph::Xmark, center, CLOSE_GLYPH, color::mix(secondary_label(), label(), hover));
    response.is_some_and(|r| r.clicked())
}

pub struct Choice<'a> {
    pub key: &'a str,
    pub title: &'a str,
    pub primary: bool,
    pub enabled: bool,
}

pub fn footer(ui: &egui::Ui, canvas: &mut Canvas, row: Rect, choices: &[Choice<'_>], live: bool) -> Option<usize> {
    let widths: Vec<f32> = choices.iter().map(|choice| buttons::width(canvas, choice.title, LEAST_BUTTON)).collect();
    let rects = buttons::trailing(row, &widths, theme::BUTTON_GAP);
    let mut pressed = None;
    for (index, (choice, rect)) in choices.iter().zip(rects).enumerate() {
        let tone = if choice.primary { buttons::Tone::Primary } else { buttons::Tone::Plain };
        let id = Id::new(("setup.button", choice.key));
        let button = buttons::Button { rect, id, title: choice.title, tone, enabled: choice.enabled };
        if buttons::button(ui, canvas, &button, live) {
            pressed = Some(index);
        }
    }
    pressed
}

#[cfg(test)]
mod tests {
    use egui::Shape;

    use super::*;

    fn with_ui(check: impl Fn(&egui::Ui, &mut Canvas)) {
        let ctx = egui::Context::default();
        notchbase_paint::font::install(&ctx);
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| check(ui, &mut Canvas::new(ui.painter())));
        output.textures_delta.clear();
    }

    #[test]
    fn card_splits_rows_evenly() {
        with_ui(|_, canvas| {
            let rect = Rect::from_min_size(pos2(16.0, 158.0), vec2(340.0, 200.0));
            let rows = card(canvas, rect, 4);
            assert_eq!(rows.len(), 4);
            assert!(rows.iter().all(|row| row.height() == 50.0 && row.width() == 340.0));
            assert_eq!(rows[3].bottom(), rect.bottom());
            assert_eq!(card(canvas, rect, 1), [rect]);
            assert!(card(canvas, rect, 0).is_empty());
        });
    }

    #[test]
    fn nothing_pressed_without_a_click() {
        with_ui(|ui, canvas| {
            let row = Rect::from_min_size(pos2(16.0, 300.0), vec2(340.0, 30.0));
            let cancel = Choice { key: "cancel", title: "Cancel", primary: false, enabled: true };
            let install = Choice { key: "install", title: "Install", primary: true, enabled: true };
            assert_eq!(footer(ui, canvas, row, &[cancel, install], true), None);
            assert_eq!(footer(ui, canvas, row, &[], true), None);
            assert!(!close(ui, canvas, pos2(340.0, 28.0), Id::new("close"), true));
        });
    }

    #[test]
    fn badge_row_draws_mark_and_text() {
        with_ui(|_, canvas| {
            let rect = Rect::from_min_size(pos2(16.0, 158.0), vec2(340.0, 50.0));
            badge_row(canvas, rect, Color32::RED, Glyph::Checkmark, &Lines::new("Installed", "Running"));
            let Shape::Circle(badge) = &canvas.shapes[0] else { panic!("a badge first") };
            assert_eq!((badge.fill, badge.radius), (Color32::RED, BADGE));
            assert_eq!(badge.center, pos2(16.0 + theme::INSET + BADGE, rect.center().y));
            assert_eq!(canvas.shapes.iter().filter(|shape| matches!(shape, Shape::Text(_))).count(), 2);
        });
    }
}
