//! Sizes nodes before they're painted. In a row, spacers, sliders, inputs and progress bars take the spare width,
//! and text and dropdowns shrink when it runs out.

use std::time::Duration;

use egui::{Vec2, vec2};
use notchbase_sdk::protocol::{Node, TextNode, TimerMode};

use super::style::text_font;
use super::{Env, shortcut};
use crate::bridge::ToolSlot;
use crate::platform::{self, LocalTime, Style};
use crate::ui::canvas::Canvas;
use crate::ui::font;
use crate::ui::metrics::{GAP, SLIDER};

pub(super) const BUTTON_H: f32 = 24.0;
pub(super) const BUTTON_PAD: f32 = 12.0;
pub(super) const BUTTON_ICON: f32 = 12.0;
pub(super) const BUTTON_ICON_GAP: f32 = 6.0;
pub(super) const TOGGLE: Vec2 = vec2(40.0, 24.0);
pub(super) const CHECK: f32 = 16.0;
pub(super) const CHECK_GAP: f32 = 8.0;
pub(super) const CHECK_ROW: f32 = 20.0;
pub(super) const CHIP_H: f32 = 24.0;
pub(super) const CHIP_GLYPH: f32 = 13.0;
pub(super) const CHIP_TEXT_GAP: f32 = 5.0;
pub(super) const CHIP_PAD: f32 = 12.0;
pub(super) const DROPDOWN_H: f32 = 22.0;
pub(super) const DROPDOWN_MIN: f32 = 96.0;
pub(super) const DROPDOWN_MAX: f32 = 168.0;
pub(super) const DROPDOWN_CHROME: f32 = 40.0;
pub(super) const SEGMENT_H: f32 = 24.0;
pub(super) const SEGMENT_MIN: f32 = 56.0;
pub(super) const SEGMENT_PAD: f32 = 20.0;
pub(super) const SLIDER_MIN: f32 = 80.0;
pub(super) const INPUT_H: f32 = 26.0;
pub(super) const INPUT_MIN: f32 = 120.0;
pub(super) const PROGRESS_H: f32 = 6.0;
pub(super) const PROGRESS_MIN: f32 = 40.0;
pub(super) const DIVIDER_H: f32 = 9.0;
pub(super) const CARD_PAD: f32 = 12.0;
pub(super) const LIST_INSET: f32 = 14.0;
pub(super) const LIST_PAD: f32 = 10.0;
pub(super) const LIST_ROW_MIN: f32 = 44.0;
pub(super) const KEYCAP_FONT: f32 = 10.5;

pub struct Measure<'a> {
    pub canvas: &'a Canvas,
    pub env: &'a Env<'a>,
    pub slot: ToolSlot,
}

impl Measure<'_> {
    pub fn size(&self, node: &Node, max: f32) -> Vec2 {
        let size = self.natural(node, max);
        vec2(size.x.min(max.max(0.0)), size.y)
    }

    fn natural(&self, node: &Node, max: f32) -> Vec2 {
        match node {
            Node::Row { children, spacing, .. } => {
                let widths = self.row_widths(children, spacing.unwrap_or(GAP), max);
                let height =
                    children.iter().zip(&widths).map(|(child, width)| self.size(child, *width).y).fold(0.0, f32::max);
                vec2(widths.iter().sum::<f32>() + gaps(children.len(), spacing.unwrap_or(GAP)), height)
            }
            Node::Column { children, spacing, .. } => {
                let sizes: Vec<Vec2> = children.iter().map(|child| self.size(child, max)).collect();
                let width = sizes.iter().map(|size| size.x).fold(0.0, f32::max);
                vec2(width, sizes.iter().map(|size| size.y).sum::<f32>() + gaps(children.len(), spacing.unwrap_or(GAP)))
            }
            Node::Frame { child, width, height, .. } => {
                let inner = self.size(child, width.unwrap_or(max));
                vec2(width.unwrap_or(inner.x), height.unwrap_or(inner.y))
            }
            Node::Card { child, padding } => {
                let pad = padding.unwrap_or(CARD_PAD);
                vec2(max, self.size(child, max - 2.0 * pad).y + 2.0 * pad)
            }
            Node::List { children } => vec2(max, children.iter().map(|child| self.list_row(child, max)).sum()),
            Node::Adaptive { notch, island } => self.size(self.pick(notch, island), max),
            Node::Keyed { child, .. } => self.size(child, max),
            Node::Pan { child, .. } => vec2(max, self.size(child, super::pan::UNBOUNDED).y),
            Node::Spacer { size } => Vec2::splat(size.unwrap_or(0.0)),
            Node::Divider => vec2(max, DIVIDER_H),
            Node::Month(_) => vec2(max, super::month::HEIGHT),
            Node::Week(week) => vec2(max, super::week::height(week)),
            Node::Canvas(canvas) => super::drawing::size(canvas, max),
            Node::Palette { colors, .. } => super::palette::size(colors.len()),
            Node::Text(text) => self.text(text, max),
            Node::Icon { size, .. }
            | Node::Ring { size, .. }
            | Node::Swatch { size, .. }
            | Node::Dot { size, .. }
            | Node::RoundButton { size, .. } => Vec2::splat(*size),
            Node::Picker { columns, .. } => {
                vec2(columns.len() as f32 * super::picker::COLUMN_W, super::picker::height())
            }
            Node::Badge { size, .. } => Vec2::splat(*size),
            Node::Button { label, icon: Some(_), .. } if label.is_empty() => Vec2::splat(BUTTON_H),
            Node::Button { label, icon, .. } => {
                let icon = if icon.is_some() { BUTTON_ICON + BUTTON_ICON_GAP } else { 0.0 };
                vec2(self.canvas.text_width(label.as_str(), font::callout()) + icon + 2.0 * BUTTON_PAD, BUTTON_H)
            }
            Node::Toggle { .. } => TOGGLE,
            Node::Checkbox { label, .. } => {
                vec2(CHECK + CHECK_GAP + self.canvas.text_width(label.as_str(), font::body()), CHECK_ROW)
            }
            Node::Chip { label, icon: Some(_), .. } if label.is_empty() => Vec2::splat(CHIP_H),
            Node::Dropdown { compact: true, .. } => Vec2::splat(CHIP_H),
            Node::Chip { label, icon, .. } => {
                let glyph = if icon.is_some() { CHIP_GLYPH + CHIP_TEXT_GAP } else { 0.0 };
                vec2(glyph + self.canvas.text_width(label.as_str(), font::section()) + 2.0 * CHIP_PAD, CHIP_H)
            }
            Node::Dropdown { options, selected, placeholder, .. } => {
                let title = dropdown_title(options, *selected, placeholder);
                let width = self.canvas.text_width(title, font::callout()) + DROPDOWN_CHROME;
                vec2(width.clamp(DROPDOWN_MIN, DROPDOWN_MAX), DROPDOWN_H)
            }
            Node::Segmented { options, .. } => {
                let widest = options
                    .iter()
                    .map(|option| self.canvas.text_width(option.as_str(), font::section()))
                    .fold(0.0, f32::max);
                vec2(options.len() as f32 * (widest + SEGMENT_PAD).max(SEGMENT_MIN), SEGMENT_H)
            }
            Node::Slider { .. } => vec2(SLIDER_MIN, SLIDER),
            Node::TextInput { .. } => vec2(INPUT_MIN, INPUT_H),
            Node::Shortcut { id } => shortcut::size(self, id),
            Node::Keycaps { keys } => shortcut::keycaps_size(self.canvas, keys),
            Node::Progress { .. } => vec2(PROGRESS_MIN, PROGRESS_H),
            Node::Image { width, height, .. } => vec2(*width, *height),
            Node::Timer { at, mode, style, .. } => {
                self.canvas.text_size(timer_label(*at, *mode, self.env.now_ms), text_font(*style))
            }
        }
    }

    pub fn pick<'n>(&self, notch: &'n Node, island: &'n Node) -> &'n Node {
        match self.env.style {
            Style::Notch => notch,
            Style::Island => island,
        }
    }

    pub fn text(&self, text: &TextNode, max: f32) -> Vec2 {
        let font = text_font(text.style);
        if text.wrap {
            self.canvas.wrapped(text.text.as_str(), font, egui::Color32::WHITE, max).size()
        } else {
            self.canvas.text_size(text.text.as_str(), font)
        }
    }

    pub fn list_row(&self, child: &Node, max: f32) -> f32 {
        (self.size(child, max - 2.0 * LIST_INSET).y + 2.0 * LIST_PAD).max(LIST_ROW_MIN)
    }

    pub fn row_widths(&self, children: &[Node], spacing: f32, max: f32) -> Vec<f32> {
        let mut widths: Vec<f32> =
            children.iter().map(|child| if flexes(child) { 0.0 } else { self.size(child, max).x }).collect();
        let room = max - gaps(children.len(), spacing);
        let fixed: f32 = widths.iter().sum();
        if fixed > room {
            let shrinkable: f32 =
                children.iter().zip(&widths).filter(|(child, _)| shrinks(child)).map(|(_, width)| width).sum();
            let cut = ((fixed - room) / shrinkable.max(1.0)).min(1.0);
            for (child, width) in children.iter().zip(widths.iter_mut()) {
                if shrinks(child) {
                    *width *= 1.0 - cut;
                }
            }
        }
        let flexing = children.iter().filter(|child| flexes(child)).count();
        if flexing > 0 {
            let spare = (room - widths.iter().sum::<f32>()).max(0.0) / flexing as f32;
            for (child, width) in children.iter().zip(widths.iter_mut()) {
                if flexes(child) {
                    *width = spare.max(flex_min(child));
                }
            }
        }
        widths
    }
}

pub fn fills(node: &Node) -> bool {
    match node {
        Node::Row { .. } | Node::Column { .. } | Node::Card { .. } | Node::List { .. } | Node::Divider => true,
        Node::Pan { .. } => true,
        Node::Canvas(canvas) => canvas.stretch,
        Node::Month(_) | Node::Week(_) => true,
        Node::Slider { .. } | Node::TextInput { .. } | Node::Progress { .. } => true,
        Node::Spacer { size } => size.is_none(),
        Node::Text(text) => text.wrap,
        Node::Frame { width, .. } => width.is_none(),
        Node::Keyed { child, .. } => fills(child),
        _ => false,
    }
}

fn flexes(node: &Node) -> bool {
    match node {
        Node::Spacer { size } => size.is_none(),
        Node::Slider { .. } | Node::TextInput { .. } | Node::Progress { .. } => true,
        Node::Keyed { child, .. } => flexes(child),
        _ => false,
    }
}

fn shrinks(node: &Node) -> bool {
    match node {
        Node::Text(_) | Node::Dropdown { compact: false, .. } => true,
        Node::Keyed { child, .. } => shrinks(child),
        _ => false,
    }
}

fn flex_min(node: &Node) -> f32 {
    match node {
        Node::Slider { .. } => SLIDER_MIN,
        Node::TextInput { .. } => INPUT_MIN * 0.5,
        Node::Progress { .. } => PROGRESS_MIN,
        _ => 0.0,
    }
}

fn gaps(count: usize, spacing: f32) -> f32 {
    count.saturating_sub(1) as f32 * spacing
}

pub fn dropdown_title<'a>(
    options: &'a [notchbase_sdk::protocol::Choice],
    selected: Option<usize>,
    placeholder: &'a str,
) -> &'a str {
    selected.and_then(|index| options.get(index)).map_or(placeholder, |choice| choice.label.as_str())
}

const DAY: i64 = 24 * 3600;

pub fn timer_label(at: u64, mode: TimerMode, now_ms: u64) -> String {
    let millis = match mode {
        TimerMode::Elapsed => now_ms.saturating_sub(at),
        TimerMode::Remaining => at.saturating_sub(now_ms).div_ceil(1000) * 1000,
        TimerMode::Clock => return wall_clock(at, now_ms, platform::local_time(), platform::clock_24h()),
    };
    notchbase_sdk::clock(Duration::from_millis(millis))
}

fn wall_clock(at: u64, now_ms: u64, local: LocalTime, full_day: bool) -> String {
    let local_now = i64::from(local.hour * 3600 + local.minute * 60 + local.second);
    let utc_now = (now_ms / 1000) as i64;
    // The local offset, worked out from the time of day and rounded to whole minutes.
    let offset = ((local_now - utc_now).rem_euclid(DAY) as f64 / 60.0).round() as i64 * 60;
    let then = ((at / 1000) as i64 + offset).rem_euclid(DAY);
    let (hour, minute) = (then / 3600, then % 3600 / 60);
    if full_day {
        return format!("{hour:02}:{minute:02}");
    }
    let half = if hour < 12 { "AM" } else { "PM" };
    format!("{}:{minute:02} {half}", (hour + 11) % 12 + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn countdowns_round_up_stopwatches_down() {
        assert_eq!(timer_label(10_000, TimerMode::Remaining, 9_001), "0:01");
        assert_eq!(timer_label(10_000, TimerMode::Remaining, 12_000), "0:00");
        assert_eq!(timer_label(1_000, TimerMode::Elapsed, 62_999), "1:01");
    }

    #[test]
    fn clock_shows_local_time() {
        let local = LocalTime { year: 2026, month: 9, day: 28, hour: 17, minute: 37, second: 10 };
        assert_eq!(wall_clock(300_000, 0, local, false), "5:42 PM");
        assert_eq!(wall_clock(300_000, 0, local, true), "17:42");
        let late = LocalTime { hour: 23, minute: 58, ..local };
        assert_eq!(wall_clock(5 * 60_000, 0, late, false), "12:03 AM");
    }

    #[test]
    fn exact_minute_never_shows_one_before() {
        let seven_pm_utc_minus_four = 23 * 3_600_000_u64;
        for lag in [1, 400, 600, 999] {
            let now_ms = seven_pm_utc_minus_four - 3_600_000 + 59 * 60_000 + 59_000 + lag;
            let local = LocalTime { year: 2026, month: 9, day: 30, hour: 18, minute: 59, second: 59 };
            assert_eq!(wall_clock(seven_pm_utc_minus_four, now_ms, local, false), "7:00 PM", "{lag}");
            assert_eq!(wall_clock(seven_pm_utc_minus_four + 30 * 60_000, now_ms, local, false), "7:30 PM");
        }
    }
}
