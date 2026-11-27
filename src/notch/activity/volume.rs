//! The volume and brightness indicator.

use std::time::{Duration, Instant};

use egui::{Align2, Rect, pos2, vec2};

use crate::notch::geometry::Geometry;
use crate::notch::{Mode, Notch};
use crate::platform::Style;
use crate::ui::canvas::Canvas;
use crate::ui::color::{label, secondary_label, white};
use crate::ui::glass::{self, Glass};
use crate::ui::glyph::{self, Glyph};
use crate::ui::{font, meter};

use super::ActivityKind;

const NOTCH_WING: f32 = 44.0;
const NOTCH_DROP: f32 = 18.0;
const ISLAND_W: f32 = 210.0;
const ISLAND_H: f32 = 37.0;
const INSET: f32 = 14.0;
const GLYPH: f32 = 15.0;
const GAP: f32 = 10.0;
const WIDEST: &str = "100";
const TRACK_H: f32 = 6.0;
const DIGITS: f32 = 12.0;
const MUTED_FILL: f32 = 0.35;
const SAMPLE_VOLUME: f32 = 0.62;
const SAMPLE_BRIGHTNESS: f32 = 0.75;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::notch) enum Indicator {
    Volume { volume: f32, muted: bool },
    Brightness(f32),
}

impl Indicator {
    pub(in crate::notch) fn level(self) -> f32 {
        match self {
            Self::Volume { volume, .. } => volume,
            Self::Brightness(level) => level,
        }
    }

    pub(in crate::notch) fn duration(self) -> Duration {
        let notices = &crate::config::current().notices;
        match self {
            Self::Volume { .. } => notices.volume(),
            Self::Brightness(_) => notices.brightness(),
        }
    }

    pub(in crate::notch) fn continues(self, shown: Self) -> bool {
        std::mem::discriminant(&self) == std::mem::discriminant(&shown)
    }

    fn muted(self) -> bool {
        matches!(self, Self::Volume { muted: true, .. })
    }

    fn glyph(self) -> Glyph {
        match self {
            Self::Volume { volume, muted } if muted || volume <= 0.001 => Glyph::SpeakerMuted,
            Self::Volume { .. } => Glyph::Speaker,
            Self::Brightness(_) => Glyph::Sun,
        }
    }
}

impl Notch {
    pub fn show_volume(&mut self, volume: f32, muted: bool) {
        self.show_indicator(Indicator::Volume { volume, muted });
    }

    pub fn show_brightness(&mut self, level: f32) {
        self.show_indicator(Indicator::Brightness(level));
    }

    fn show_indicator(&mut self, indicator: Indicator) {
        if self.preview.is_some() {
            return;
        }
        let now = Instant::now();
        if self.carries_indicator() {
            self.show_indicator_on_music(indicator, now);
            return;
        }
        let kind = ActivityKind::Indicator(indicator);
        if let Some(shown) = self.shown_indicator() {
            if indicator.continues(shown) {
                self.level_meter.follow(indicator.level());
            } else {
                self.level_meter.snap(indicator.level());
            }
            self.activity.kind = Some(kind);
            self.activity.until = Some(now + indicator.duration());
            return;
        }
        self.level_meter.snap(indicator.level());
        self.start_activity(kind, now);
    }

    pub(in crate::notch) fn pin_sample_volume(&mut self) -> ActivityKind {
        self.level_meter.snap(SAMPLE_VOLUME);
        ActivityKind::Indicator(Indicator::Volume { volume: SAMPLE_VOLUME, muted: false })
    }

    pub(in crate::notch) fn pin_sample_brightness(&mut self) -> ActivityKind {
        self.level_meter.snap(SAMPLE_BRIGHTNESS);
        ActivityKind::Indicator(Indicator::Brightness(SAMPLE_BRIGHTNESS))
    }

    pub(super) fn shows_indicator(&self) -> bool {
        self.shown_indicator().is_some()
    }

    fn shown_indicator(&self) -> Option<Indicator> {
        let Some(ActivityKind::Indicator(shown)) = self.activity.kind else { return None };
        (self.activity.is_live() && matches!(self.mode, Mode::Activity | Mode::Peek)).then_some(shown)
    }

    pub(in crate::notch) fn indicator_is_moving(&self) -> bool {
        (self.shows_indicator() && self.level_meter.is_moving()) || self.strip_is_moving()
    }

    pub(super) fn indicator_geometry(&self) -> Geometry {
        match self.style {
            Style::Notch => {
                Geometry { w: self.notch.x + 2.0 * NOTCH_WING, h: self.notch.y + NOTCH_DROP, r: 12.0, ear: 6.0 }
            }
            Style::Island => Geometry { w: ISLAND_W, h: ISLAND_H, r: ISLAND_H * 0.5, ear: 0.0 },
        }
    }

    pub(super) fn paint_indicator(&self, canvas: &mut Canvas, body: Rect, indicator: Indicator) {
        let reading = self.level_meter.reading();
        match self.style {
            Style::Island => {
                let row = IndicatorRow {
                    left: body.left() + INSET,
                    right: body.right() - INSET,
                    y: body.center().y,
                    glyph: GLYPH,
                    column: GLYPH,
                    track: TRACK_H,
                    digits: DIGITS,
                };
                paint_indicator_row(canvas, &row, reading, indicator);
            }
            Style::Notch => {
                let lane_y = body.top() + self.notch.y * 0.5;
                let glyph_at = pos2(self.lead(body.left() + INSET) + GLYPH * 0.5, lane_y);
                glyph::paint(canvas, indicator.glyph(), glyph_at, GLYPH, label());
                let percent_at = pos2(body.right() - INSET, lane_y);
                let percent = percent(indicator.level());
                canvas.text(percent_at, Align2::RIGHT_CENTER, percent, font::mono(DIGITS), secondary_label());
                let y = body.top() + self.notch.y + NOTCH_DROP * 0.5 - 1.0;
                let track = Rect::from_min_max(
                    pos2(body.left() + INSET, y - TRACK_H * 0.5),
                    pos2(body.right() - INSET, y + TRACK_H * 0.5),
                );
                paint_bar(canvas, track, reading, indicator.muted());
            }
        }
    }
}

pub(in crate::notch) struct IndicatorRow {
    pub(in crate::notch) left: f32,
    pub(in crate::notch) right: f32,
    pub(in crate::notch) y: f32,
    pub(in crate::notch) glyph: f32,
    pub(in crate::notch) column: f32,
    pub(in crate::notch) track: f32,
    pub(in crate::notch) digits: f32,
}

pub(in crate::notch) fn paint_indicator_row(
    canvas: &mut Canvas,
    row: &IndicatorRow,
    reading: meter::Reading,
    shown: Indicator,
) {
    let glyph_at = pos2(row.left + row.column * 0.5, row.y);
    glyph::paint(canvas, shown.glyph(), glyph_at, row.glyph, label());
    let percent = percent(shown.level());
    canvas.text(pos2(row.right, row.y), Align2::RIGHT_CENTER, percent, font::mono(row.digits), secondary_label());
    let widest = canvas.text_width(WIDEST, font::mono(row.digits));
    let (left, right) = (row.left + row.column + GAP, row.right - widest - GAP);
    let track = Rect::from_min_max(pos2(left, row.y - row.track * 0.5), pos2(right, row.y + row.track * 0.5));
    paint_bar(canvas, track, reading, shown.muted());
}

fn percent(level: f32) -> String {
    format!("{}", (level * 100.0).round() as u32)
}

fn paint_bar(canvas: &mut Canvas, track: Rect, reading: meter::Reading, muted: bool) {
    if track.width() <= track.height() {
        return;
    }
    glass::paint(canvas, track, track.height() * 0.5, Glass::CONTROL);
    let h = track.height();
    let width = (track.width() * reading.level.clamp(0.0, 1.0)).max(h);
    let alpha = if muted { 0.92 * MUTED_FILL } else { 0.92 };
    canvas.rect(Rect::from_min_size(track.min, vec2(width, h)), h * 0.5, white(alpha));
}

#[cfg(test)]
mod tests {
    use egui::vec2;

    use super::*;
    use crate::notch::tests::fixtures::{AWAY, island, ms, settle};

    #[test]
    fn changes_update_one_indicator() {
        let mut n = island();
        n.show_volume(0.4, false);
        assert!(n.shows_indicator());
        let first_until = n.activity.until.expect("live");
        settle(&mut n);
        assert_eq!(n.silhouette().rect(), n.body_rect(n.indicator_geometry()));
        std::thread::sleep(ms(5));
        n.show_volume(0.42, false);
        assert!(n.activity.until.expect("live") > first_until);
        assert_eq!(n.activity.kind, Some(ActivityKind::Indicator(Indicator::Volume { volume: 0.42, muted: false })));
        let later = n.activity.until.expect("live") + ms(10);
        n.tick(later, Some(AWAY));
        assert!(!n.shows_indicator());
        assert_eq!(n.mode, Mode::Idle);
    }

    #[test]
    fn brightness_takes_over() {
        let mut n = island();
        n.show_volume(0.4, false);
        settle(&mut n);
        let shape = n.silhouette().rect();
        n.show_brightness(0.75);
        assert_eq!(n.activity.kind, Some(ActivityKind::Indicator(Indicator::Brightness(0.75))));
        assert_eq!(n.level_meter.reading().level, 0.75, "the meter jumps to the other level instead of sliding");
        assert_eq!(n.silhouette().rect(), shape);
        n.show_brightness(0.8125);
        assert!(n.level_meter.is_moving());
        let later = n.activity.until.expect("live") + ms(10);
        n.tick(later, Some(AWAY));
        assert_eq!(n.mode, Mode::Idle);
    }

    #[test]
    fn indicator_clears_camera() {
        let n = Notch::with_preview(Style::Notch, Some(vec2(185.0, 32.0)), None);
        let g = n.indicator_geometry();
        let wing = (g.w - 185.0) * 0.5;
        assert!(INSET + GLYPH + 8.0 <= wing);
        assert!(g.h > 32.0);
    }

    #[test]
    fn bar_length_ignores_digits() {
        let ctx = egui::Context::default();
        crate::ui::font::install(&ctx);
        let row = IndicatorRow {
            left: 20.0,
            right: 260.0,
            y: 20.0,
            glyph: GLYPH,
            column: GLYPH,
            track: TRACK_H,
            digits: DIGITS,
        };
        let mut ends = Vec::new();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            for level in [0.04, 0.4, 1.0] {
                let mut canvas = Canvas::new(ui.painter());
                let reading = meter::Reading { level, ..meter::Reading::default() };
                paint_indicator_row(&mut canvas, &row, reading, Indicator::Volume { volume: level, muted: false });
                let bars = canvas.shapes.iter().filter(|shape| !matches!(shape, egui::Shape::Text(_)));
                ends.push(bars.map(|shape| shape.visual_bounding_rect().right()).fold(0.0, f32::max));
            }
        });
        output.textures_delta.clear();
        assert!(ends.windows(2).all(|pair| (pair[0] - pair[1]).abs() < 0.01), "{ends:?}");
    }
}
