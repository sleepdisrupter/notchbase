//! How a boxed field looks: glass with an accent ring while focused, the caret or selection, and dots for a secret.

use egui::{Align2, Rect, Stroke, pos2, vec2};

use crate::ui::canvas::Canvas;
use crate::ui::color::{self, label, tertiary_label};
use crate::ui::glass::{self, Glass};
use crate::ui::{controls, font};

use super::{Focus, MASK, MOST_MASKED, bare, masked};

const RADIUS: f32 = 8.0;
const PAD: f32 = 10.0;
const CARET_H: f32 = 14.0;
const CARET_W: f32 = 1.5;
const RING: f32 = 1.5;
const SELECTION_PAD: f32 = 3.0;
const SELECTION_ALPHA: f32 = 0.35;

pub(super) struct Look<'a> {
    pub(super) value: &'a str,
    pub(super) prefix: &'a str,
    pub(super) placeholder: &'a str,
    pub(super) secret: bool,
}

pub(super) fn paint(canvas: &mut Canvas, rect: Rect, hover: f32, focus: Option<&Focus>, look: &Look) {
    let Look { value, prefix, placeholder, secret } = *look;
    glass::paint(canvas, rect, RADIUS, Glass::lerp(Glass::CONTROL, Glass::TILE_HOVER, hover * 0.6));
    if focus.is_some() {
        canvas.rect_stroke(rect, RADIUS, Stroke::new(RING, color::ACCENT));
    }
    let line = focus.map(|focus| &focus.line);
    let selected = line.is_some_and(|line| line.all);
    let shown = shown(line.map_or(value, |line| &line.text), line.is_some(), prefix, secret);
    let room = rect.width() - 2.0 * PAD;
    let left = rect.left() + PAD;
    if shown.is_empty() {
        let hint = controls::truncate(canvas, placeholder, font::body(), room);
        canvas.text(pos2(left, rect.center().y), Align2::LEFT_CENTER, hint, font::body(), tertiary_label());
    }
    let Some(line) = line else {
        let text = controls::truncate(canvas, &shown, font::body(), room);
        canvas.text(pos2(left, rect.center().y), Align2::LEFT_CENTER, text, font::body(), label());
        return;
    };
    let caret = prefix.len() + caret_in_shown(&line.text, line.caret, secret);
    let width = |text: &str| canvas.text_width(text, font::body());
    let seen = bare::window(&shown, caret.min(shown.len()), room, width);
    let before = width(&shown[seen.start..caret.min(shown.len())]);
    let text = shown[seen].to_owned();
    if selected {
        let band = Rect::from_min_size(
            pos2(left - 1.0, rect.center().y - CARET_H / 2.0 - SELECTION_PAD / 2.0),
            vec2(width(&text) + 2.0, CARET_H + SELECTION_PAD),
        );
        canvas.rect(band, SELECTION_PAD, color::ACCENT.gamma_multiply(SELECTION_ALPHA));
    }
    canvas.text(pos2(left, rect.center().y), Align2::LEFT_CENTER, text, font::body(), label());
    if !selected {
        let caret = Rect::from_center_size(pos2(left + before + 1.0, rect.center().y), vec2(CARET_W, CARET_H));
        canvas.rect(caret, 0.0, color::ACCENT);
    }
}

/// The caret's place among a secret's dots. The dots stop at 24, so a long secret parks it at the end.
fn caret_in_shown(raw: &str, caret: usize, secret: bool) -> usize {
    if !secret {
        return caret;
    }
    let dots = raw[..caret.min(raw.len())].chars().count().min(MOST_MASKED);
    dots * MASK.len_utf8()
}

fn shown(raw: &str, typing: bool, prefix: &str, secret: bool) -> String {
    let body = if secret { masked(raw) } else { raw.to_owned() };
    if raw.is_empty() && !typing { body } else { format!("{prefix}{body}") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_leads_text_not_placeholder() {
        assert_eq!(shown("john", false, "@", false), "@john");
        assert_eq!(shown("", false, "@", false), "");
        assert_eq!(shown("", true, "@", false), "@");
        assert_eq!(shown("ab", true, "", true), "\u{2022}\u{2022}");
    }

    #[test]
    fn secret_caret_sits_between_dots() {
        let secret = "pässword";
        let dots = shown(secret, true, "", true);
        assert_eq!(&dots[..caret_in_shown(secret, 0, true)], "");
        assert_eq!(dots[..caret_in_shown(secret, "pä".len(), true)].chars().count(), 2);
        assert_eq!(caret_in_shown(secret, secret.len(), true), dots.len());
        assert_eq!(caret_in_shown("john", 2, false), 2);
        let long = "x".repeat(40);
        assert_eq!(caret_in_shown(&long, 30, true), masked(&long).len(), "past the last dot it rests at the end");
    }
}
