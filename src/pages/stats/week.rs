//! This week: a column for each day, and how many days hit the limit.

use egui::{Align2, Color32, Rect, pos2};

use super::tone;
use crate::assistant::Provider;
use crate::pages::PageCtx;
use crate::pages::settings::{INSET, SECTION_H, section};
use crate::stats::{DAYS, Day, Week};
use crate::ui::{color, font};

const TITLE: &str = "This week";
const COLUMN_H: f32 = 24.0;
const LABEL_GAP: f32 = 3.0;
const LABEL_H: f32 = 11.0;
const STRIP_H: f32 = COLUMN_H + LABEL_GAP + LABEL_H;
const COLUMN_W: f32 = 12.0;
const COLUMN_SHARE: f32 = 0.6;
const RADIUS: f32 = 3.0;
const SHORTEST: f32 = 3.0;
const HAIRLINE: f32 = 1.0;
const PAST: f32 = 0.7;
const HEIGHT: f32 = SECTION_H + STRIP_H;

pub(super) fn height(week: &Week) -> f32 {
    if week.is_empty() { 0.0 } else { HEIGHT }
}

pub(super) fn paint(ctx: &mut PageCtx<'_>, top: f32, week: &Week, assistant: Provider) {
    if week.is_empty() {
        return;
    }
    let header = section(ctx, top, TITLE);
    let summary = summary(week.days_at_limit());
    let at = pos2(header.right() - INSET, header.center().y);
    ctx.canvas.text(at, Align2::RIGHT_CENTER, summary, font::caption(), color::tertiary_label());
    let (left, right) = (ctx.rect.left() + INSET, ctx.rect.right() - INSET);
    let slot = (right - left) / DAYS as f32;
    let width = COLUMN_W.min(slot * COLUMN_SHARE);
    let baseline = header.bottom() + COLUMN_H;
    for (i, day) in week.days.iter().enumerate() {
        let today = i == DAYS - 1;
        let x = left + slot * (i as f32 + 0.5);
        let (rect, radius, fill) = column(day, x, baseline, width, assistant, today);
        ctx.canvas.rect(rect, radius, fill);
        let tone = if today { color::secondary_label() } else { color::tertiary_label() };
        let label_y = baseline + LABEL_GAP + LABEL_H * 0.5;
        ctx.canvas.text(pos2(x, label_y), Align2::CENTER_CENTER, day.weekday, font::caption(), tone);
    }
}

fn column(day: &Day, x: f32, baseline: f32, width: f32, assistant: Provider, today: bool) -> (Rect, f32, Color32) {
    let Some(peak) = day.peak else {
        let line = Rect::from_min_max(pos2(x - width * 0.5, baseline - HAIRLINE), pos2(x + width * 0.5, baseline));
        return (line, 0.0, color::tertiary_label());
    };
    let tall = (peak.clamp(0.0, 1.0) * COLUMN_H).max(SHORTEST);
    let rect = Rect::from_min_max(pos2(x - width * 0.5, baseline - tall), pos2(x + width * 0.5, baseline));
    let fill = tone::usage(assistant, peak);
    (rect, RADIUS.min(tall * 0.5), if today { fill } else { fill.gamma_multiply(PAST) })
}

fn summary(days: usize) -> String {
    match days {
        0 => "Never hit the limit".to_owned(),
        1 => "Hit the limit 1 day".to_owned(),
        n => format!("Hit the limit {n} days"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(peak: Option<f32>) -> Day {
        Day { peak, hit_limit: false, weekday: 'M' }
    }

    #[test]
    fn empty_week_takes_no_room() {
        assert_eq!(height(&Week::default()), 0.0);
        let mut week = Week::default();
        week.days[3] = day(Some(0.0));
        assert_eq!(height(&week), SECTION_H + COLUMN_H + LABEL_GAP + LABEL_H);
        assert!((60.0..=68.0).contains(&height(&week)), "about 64 points");
    }

    #[test]
    fn columns_rise_from_one_baseline() {
        let (full, _, _) = column(&day(Some(1.0)), 50.0, 100.0, COLUMN_W, Provider::Claude, true);
        assert_eq!((full.top(), full.bottom(), full.center().x), (100.0 - COLUMN_H, 100.0, 50.0));
        let (tiny, radius, _) = column(&day(Some(0.0)), 50.0, 100.0, COLUMN_W, Provider::Claude, true);
        assert_eq!((tiny.height(), radius), (SHORTEST, SHORTEST * 0.5));
        let (line, _, tone) = column(&day(None), 50.0, 100.0, COLUMN_W, Provider::Claude, false);
        assert_eq!((line.height(), line.bottom(), tone), (HAIRLINE, 100.0, color::tertiary_label()));
    }

    #[test]
    fn today_is_brighter() {
        let (_, _, today) = column(&day(Some(0.5)), 0.0, 0.0, COLUMN_W, Provider::Codex, true);
        let (_, _, before) = column(&day(Some(0.5)), 0.0, 0.0, COLUMN_W, Provider::Codex, false);
        assert_eq!(today, tone::usage(Provider::Codex, 0.5));
        assert!(before.a() < today.a());
    }

    #[test]
    fn summary_counts_limit_days() {
        assert_eq!(summary(0), "Never hit the limit");
        assert_eq!(summary(1), "Hit the limit 1 day");
        assert_eq!(summary(2), "Hit the limit 2 days");
    }
}
