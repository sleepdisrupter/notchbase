//! A bare field, just text and a caret, used for the search line in Find. It never gives up the keyboard by itself.

use std::ops::Range;

use egui::{Align2, FontId, Id, Rect, pos2, vec2};

use crate::ui::canvas::Canvas;
use crate::ui::color::{self, label, tertiary_label, white};

use super::edit::Line;
use super::{Edit, Focus};

const CARET_W: f32 = 1.5;
const CARET_H: f32 = 17.0;
const CARET_NUDGE: f32 = 0.5;
const BAND_PAD: f32 = 2.0;
const BAND_RADIUS: f32 = 4.0;
const BAND_ALPHA: f32 = 0.24;
const HINT_AFTER_CARET: f32 = 4.0;

pub struct Bare<'a> {
    pub id: Id,
    pub rect: Rect,
    pub text_left: f32,
    pub text_right: f32,
    pub placeholder: &'a str,
    pub font: FontId,
}

pub fn bare(ui: &egui::Ui, canvas: &mut Canvas, field: &Bare<'_>, focus: &mut Option<Focus>) -> Vec<Edit> {
    let Some(current) = focus.as_mut().filter(|current| current.id == field.id) else {
        hint(canvas, field);
        return Vec::new();
    };
    let edits = current.take(ui, field.rect);
    paint(canvas, field, &current.line);
    edits
}

impl Focus {
    pub fn set_text(&mut self, text: &str) {
        self.line.set(text);
    }
}

/// The part of `text` that fits in `room`, always keeping the caret in view.
pub(super) fn window(text: &str, caret: usize, room: f32, width: impl Fn(&str) -> f32) -> Range<usize> {
    let mut start = 0;
    while start < caret && width(&text[start..caret]) > room {
        start += text[start..].chars().next().map_or(1, char::len_utf8);
    }
    let mut end = text.len();
    while end > caret && width(&text[start..end]) > room {
        end = text[..end].char_indices().next_back().map_or(caret, |(index, _)| index);
    }
    start..end
}

fn hint(canvas: &mut Canvas, field: &Bare<'_>) {
    let at = pos2(field.text_left + HINT_AFTER_CARET, field.rect.center().y);
    canvas.text(at, Align2::LEFT_CENTER, field.placeholder, field.font.clone(), tertiary_label());
}

fn paint(canvas: &mut Canvas, field: &Bare<'_>, line: &Line) {
    let y = field.rect.center().y;
    if line.text.is_empty() {
        hint(canvas, field);
        caret(canvas, field.text_left, y);
        return;
    }
    let room = (field.text_right - field.text_left).max(0.0);
    let width = |text: &str| canvas.text_width(text, field.font.clone());
    let seen = window(&line.text, line.caret, room, width);
    let before = width(&line.text[seen.start..line.caret]);
    let text = &line.text[seen];
    if line.all {
        let band = Rect::from_min_size(
            pos2(field.text_left - BAND_PAD, y - CARET_H * 0.5 - BAND_PAD),
            vec2(width(text) + 2.0 * BAND_PAD, CARET_H + 2.0 * BAND_PAD),
        );
        canvas.rect(band, BAND_RADIUS, white(BAND_ALPHA));
    }
    canvas.text(pos2(field.text_left, y), Align2::LEFT_CENTER, text, field.font.clone(), label());
    if !line.all {
        caret(canvas, field.text_left + before, y);
    }
}

fn caret(canvas: &mut Canvas, x: f32, y: f32) {
    let rect = Rect::from_center_size(pos2(x + CARET_NUDGE, y), vec2(CARET_W, CARET_H));
    canvas.rect(rect, CARET_W * 0.5, color::PEARL);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::field::begin_selected;
    use crate::ui::font;

    fn letters(text: &str) -> f32 {
        text.chars().count() as f32
    }

    #[test]
    fn setting_text_drops_selection() {
        let mut focus = None;
        begin_selected(&mut focus, Id::new("find"), "cargo", Rect::NOTHING, &mut Vec::new());
        let focus = focus.as_mut().expect("focused");
        focus.set_text("");
        assert_eq!((focus.text(), focus.line.all), ("", false));
        focus.set_text("car");
        assert_eq!((focus.text(), focus.line.caret), ("car", 3));
    }

    #[test]
    fn long_text_window_holds_caret() {
        let text = "abcdefghij";
        assert_eq!(window(text, 10, 4.0, letters), 6..10, "the end shows while typing at the end");
        assert_eq!(window(text, 2, 4.0, letters), 0..4, "the start shows with the caret near it");
        assert_eq!(window(text, 6, 4.0, letters), 2..6);
        assert_eq!(window(text, 5, 20.0, letters), 0..10, "a short text shows whole");
        assert_eq!(window("", 0, 4.0, letters), 0..0);
    }

    #[test]
    fn bare_field_never_lets_go() {
        let ctx = egui::Context::default();
        font::install(&ctx);
        let id = Id::new("find");
        let mut focus = None;
        begin_selected(&mut focus, id, "", Rect::NOTHING, &mut Vec::new());
        let events = vec![
            egui::Event::Text("car".into()),
            egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
        ];
        let mut edits = Vec::new();
        let mut output = ctx.run_ui(egui::RawInput { events, ..Default::default() }, |ui| {
            let mut canvas = Canvas::new(ui.painter());
            let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(300.0, 38.0));
            let field = Bare { id, rect, text_left: 36.0, text_right: 260.0, placeholder: "Find", font: font::body() };
            edits = bare(ui, &mut canvas, &field, &mut focus);
        });
        output.textures_delta.clear();
        assert_eq!(edits, [Edit::Typed("car".into()), Edit::Cancelled]);
        assert!(focus.is_some_and(|focus| focus.text() == "car"), "escape is reported, the focus is kept");
    }

    #[test]
    fn placeholder_starts_after_caret() {
        let ctx = egui::Context::default();
        font::install(&ctx);
        let id = Id::new("find");
        let mut focus = None;
        begin_selected(&mut focus, id, "", Rect::NOTHING, &mut Vec::new());
        let (mut caret, mut hint) = (Rect::NOTHING, Rect::NOTHING);
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let mut canvas = Canvas::new(ui.painter());
            let mark = canvas.mark();
            let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(300.0, 38.0));
            let field = Bare { id, rect, text_left: 36.0, text_right: 260.0, placeholder: "Find", font: font::body() };
            bare(ui, &mut canvas, &field, &mut focus);
            for shape in canvas.since(mark).iter() {
                match shape {
                    egui::Shape::Text(text) => hint = text.visual_bounding_rect(),
                    egui::Shape::Rect(rect) => caret = rect.rect,
                    _ => {}
                }
            }
        });
        output.textures_delta.clear();
        assert!(caret.is_positive() && hint.is_positive(), "{caret:?} {hint:?}");
        assert!(hint.left() >= caret.right() + 2.0, "the caret {caret:?} stays clear of the first letter {hint:?}");
    }
}
