//! The status in the title: Copying while the clipboard is behind, then Copied or Not copied for the current picture.

use egui::{Align2, Pos2, pos2};

use crate::ui::canvas::{Canvas, scale_about};
use crate::ui::glyph::{self, Glyph};
use crate::ui::spring::{Spring, UNIT_PRECISION};
use crate::ui::{color, font, motion};

const COPIED: &str = "Copied";
const COPYING: &str = "Copying…";
const NOT_COPIED: &str = "Not copied";
const MARK: f32 = 11.0;
const MARK_GAP: f32 = 5.0;
const LANDS_FROM: f32 = 0.8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Clipboard {
    Untouched,
    Holds(u64),
    Refused(u64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Reading {
    Copied,
    Copying,
    NotCopied,
}

pub(super) struct Status {
    clipboard: Clipboard,
    landed: Spring,
}

impl Default for Status {
    fn default() -> Self {
        Self { clipboard: Clipboard::Untouched, landed: Spring::new(1.0, motion::BOUNCY, UNIT_PRECISION) }
    }
}

impl Status {
    pub(super) fn holds(&mut self, copied: u64, current: u64) {
        self.clipboard = Clipboard::Holds(copied);
        if copied == current {
            self.landed.value = LANDS_FROM;
            self.landed.animate_to(1.0, motion::BOUNCY);
        }
    }

    pub(super) fn refused(&mut self, revision: u64) {
        if self.latest_news() <= Some(revision) {
            self.clipboard = Clipboard::Refused(revision);
        }
    }

    fn latest_news(&self) -> Option<u64> {
        match self.clipboard {
            Clipboard::Untouched => None,
            Clipboard::Holds(revision) | Clipboard::Refused(revision) => Some(revision),
        }
    }

    pub(super) fn step(&mut self, dt: f32) {
        self.landed.step(dt);
    }

    pub(super) fn is_settled(&self) -> bool {
        self.landed.is_settled()
    }

    pub(super) fn reading(&self, current: u64) -> Option<Reading> {
        match self.clipboard {
            Clipboard::Holds(revision) if revision == current => Some(Reading::Copied),
            Clipboard::Refused(revision) if revision == current => Some(Reading::NotCopied),
            Clipboard::Untouched if current == 0 => None,
            _ => Some(Reading::Copying),
        }
    }

    pub(super) fn paint(&self, canvas: &mut Canvas, right_center: Pos2, reading: Option<Reading>) {
        let Some(reading) = reading else { return };
        let mark = canvas.mark();
        let (text, sign) = match reading {
            Reading::Copied => (COPIED, Some((Glyph::Checkmark, color::GREEN))),
            Reading::Copying => (COPYING, None),
            Reading::NotCopied => (NOT_COPIED, Some((Glyph::Exclamation, color::RED))),
        };
        let words = canvas.text(right_center, Align2::RIGHT_CENTER, text, font::callout(), color::secondary_label());
        let mut bounds = words;
        if let Some((sign, ink)) = sign {
            let at = pos2(words.left() - MARK_GAP - MARK / 2.0, right_center.y);
            glyph::paint(canvas, sign, at, MARK, ink);
            bounds = bounds.union(egui::Rect::from_center_size(at, egui::Vec2::splat(MARK)));
        }
        let pop = scale_about(bounds.center(), self.landed.value);
        canvas.since(mark).iter_mut().for_each(|shape| shape.transform(pop));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_claimed_before_first_copy() {
        assert_eq!(Status::default().reading(0), None);
        assert_eq!(Status::default().reading(1), Some(Reading::Copying));
    }

    #[test]
    fn copied_only_for_current_picture() {
        let mut status = Status::default();
        status.holds(3, 3);
        assert_eq!(status.reading(3), Some(Reading::Copied));
        assert_eq!(status.reading(4), Some(Reading::Copying));
        status.holds(2, 4);
        assert_eq!(status.reading(4), Some(Reading::Copying), "an older copy landing late claims nothing");
        status.holds(0, 0);
        assert_eq!(status.reading(0), Some(Reading::Copied));
    }

    #[test]
    fn refusal_shown_until_picture_changes() {
        let mut status = Status::default();
        status.refused(5);
        assert_eq!(status.reading(5), Some(Reading::NotCopied));
        assert_eq!(status.reading(6), Some(Reading::Copying));
    }

    #[test]
    fn refusal_speaks_for_its_picture() {
        let mut status = Status::default();
        status.refused(1);
        assert_eq!(status.reading(2), Some(Reading::Copying), "the newer picture has not been tried yet");
        status.holds(2, 2);
        assert_eq!(status.reading(2), Some(Reading::Copied));
        status.refused(1);
        assert_eq!(status.reading(2), Some(Reading::Copied), "old news never hides a newer copy");
        status.refused(2);
        assert_eq!(status.reading(2), Some(Reading::NotCopied));
    }

    #[test]
    fn landing_copy_pops_then_rests() {
        let mut status = Status::default();
        assert!(status.is_settled());
        status.holds(1, 1);
        assert!(!status.is_settled());
        (0..240).for_each(|_| status.step(1.0 / 120.0));
        assert!(status.is_settled() && (status.landed.value - 1.0).abs() < 1e-3);
        status.holds(1, 2);
        assert!(status.is_settled(), "a stale copy moves nothing");
    }
}
