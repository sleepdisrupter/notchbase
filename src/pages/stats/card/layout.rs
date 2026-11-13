//! Where each row of the usage card goes and how tall the card is.

use egui::{Rect, pos2, vec2};

use crate::assistant::Provider;
use crate::stats::{PlanUsage, UsageWindow};
use crate::ui::metrics;

pub(super) const PAD: f32 = metrics::CARD_RADIUS;
pub(super) const TOP: f32 = 12.0;
pub(super) const TITLE_H: f32 = 16.0;
pub(super) const LINE_H: f32 = 14.0;
pub(super) const MESSAGE_GAP: f32 = 8.0;
const TITLE_GAP: f32 = 10.0;
const BAR_GAP: f32 = 6.0;
const BAR_H: f32 = 6.0;
const ROW_H: f32 = LINE_H + BAR_GAP + BAR_H;
const ROW_GAP: f32 = 12.0;
const BOTTOM: f32 = 14.0;
const MESSAGE_H: f32 = TOP + TITLE_H + MESSAGE_GAP + LINE_H + BOTTOM;

pub(super) const LIMITS: [&str; 2] = ["5-hour", "Weekly"];
const SIGNED_OUT: &str = "Sign in to Claude or Claude Code to see usage";
const NO_CODEX: &str = "Use Codex on this computer to see ChatGPT usage";
const EXPIRED: &str = "Open the Claude app or run \u{201C}claude\u{201D} to refresh usage";
const NO_LIMITS: &str = "No usage limits on this plan";

pub(super) type Windows<'a> = [Option<&'a UsageWindow>; LIMITS.len()];

pub(super) enum Body<'a> {
    Placeholder,
    Limits(Windows<'a>),
    Message(&'a str),
}

impl<'a> Body<'a> {
    pub(super) fn of(usage: &'a PlanUsage, assistant: Provider) -> Self {
        match usage {
            PlanUsage::Loading => Body::Placeholder,
            PlanUsage::Ready { five_hour: None, weekly: None, .. } => Body::Message(NO_LIMITS),
            PlanUsage::Ready { five_hour, weekly, .. } => Body::Limits([five_hour.as_ref(), weekly.as_ref()]),
            PlanUsage::SignedOut => Body::Message(match assistant {
                Provider::Claude => SIGNED_OUT,
                Provider::Codex => NO_CODEX,
            }),
            PlanUsage::Expired => Body::Message(EXPIRED),
            PlanUsage::Error(message) => Body::Message(message),
        }
    }

    pub(super) fn height(&self) -> f32 {
        match self {
            Body::Placeholder => rows_height(LIMITS.len()),
            Body::Limits(windows) => rows_height(shown(windows).count()),
            Body::Message(_) => MESSAGE_H,
        }
    }
}

pub(super) fn shown<'a>(windows: &Windows<'a>) -> impl Iterator<Item = (usize, &'a UsageWindow)> {
    windows.iter().enumerate().filter_map(|(limit, window)| window.map(|window| (limit, window)))
}

fn rows_height(rows: usize) -> f32 {
    TOP + TITLE_H + TITLE_GAP + rows as f32 * (ROW_H + ROW_GAP) - ROW_GAP + BOTTOM
}

pub(super) struct RowRect {
    pub(super) line_y: f32,
    pub(super) bar: Rect,
}

pub(super) fn row_rect(card: Rect, slot: usize) -> RowRect {
    let top = card.top() + TOP + TITLE_H + TITLE_GAP + slot as f32 * (ROW_H + ROW_GAP);
    let bar =
        Rect::from_min_size(pos2(card.left() + PAD, top + LINE_H + BAR_GAP), vec2(card.width() - 2.0 * PAD, BAR_H));
    RowRect { line_y: top + LINE_H * 0.5, bar }
}

#[cfg(test)]
mod tests {
    use std::time::UNIX_EPOCH;

    use super::*;

    fn window(used: f32) -> Option<UsageWindow> {
        Some(UsageWindow { used, resets_at: None, estimated: false, minutes: None })
    }

    fn height(claude: &PlanUsage) -> f32 {
        Body::of(claude, Provider::Claude).height()
    }

    fn ready(five_hour: Option<UsageWindow>, weekly: Option<UsageWindow>) -> PlanUsage {
        PlanUsage::Ready { five_hour, weekly, fetched_at: UNIX_EPOCH }
    }

    fn limits(claude: &PlanUsage) -> Vec<(usize, f32)> {
        match Body::of(claude, Provider::Claude) {
            Body::Limits(windows) => shown(&windows).map(|(limit, w)| (limit, w.used)).collect(),
            _ => Vec::new(),
        }
    }

    #[test]
    fn only_present_windows_get_rows() {
        assert_eq!(limits(&ready(window(0.1), window(0.2))), vec![(0, 0.1), (1, 0.2)]);
        assert_eq!(limits(&ready(None, window(0.2))), vec![(1, 0.2)]);
        assert_eq!(limits(&ready(window(0.1), None)), vec![(0, 0.1)]);
        assert!(matches!(Body::of(&ready(None, None), Provider::Claude), Body::Message(NO_LIMITS)));
        assert!(matches!(Body::of(&PlanUsage::Loading, Provider::Claude), Body::Placeholder));
        assert!(matches!(Body::of(&PlanUsage::Error("x".into()), Provider::Claude), Body::Message("x")));
    }

    #[test]
    fn missing_usage_explained() {
        assert!(matches!(Body::of(&PlanUsage::SignedOut, Provider::Claude), Body::Message(SIGNED_OUT)));
        assert!(matches!(Body::of(&PlanUsage::SignedOut, Provider::Codex), Body::Message(NO_CODEX)));
    }

    #[test]
    fn height_follows_the_rows_shown() {
        let two = height(&ready(window(0.1), window(0.2)));
        let one = height(&ready(None, window(0.2)));
        assert_eq!(two, TOP + TITLE_H + TITLE_GAP + 2.0 * ROW_H + ROW_GAP + BOTTOM);
        assert_eq!(height(&PlanUsage::Loading), two, "loading keeps room for both rows");
        assert_eq!(height(&ready(window(0.1), None)), one);
        assert_eq!(two - one, ROW_H + ROW_GAP);
        assert_eq!(height(&ready(None, None)), MESSAGE_H);
        assert_eq!(height(&PlanUsage::SignedOut), MESSAGE_H);
        assert_eq!(height(&PlanUsage::Error(String::new())), MESSAGE_H);
    }

    #[test]
    fn rows_end_at_the_card_bottom() {
        for (claude, rows) in [(ready(window(0.1), window(0.2)), 2), (ready(None, window(0.2)), 1)] {
            let card = Rect::from_min_size(pos2(0.0, 0.0), vec2(340.0, height(&claude)));
            assert_eq!(row_rect(card, rows - 1).bar.bottom() + BOTTOM, card.bottom());
        }
    }
}
