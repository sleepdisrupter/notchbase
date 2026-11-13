//! The usage card: how much of each limit the assistant has used, and when each one resets.

mod layout;
mod paint;

use std::f32::consts::TAU;
use std::time::{Duration, Instant, SystemTime};

use egui::{Align2, Id, Pos2, Rect, Response, Sense, pos2, vec2};

use super::format::{Wall, format_ago, usage_detail, window_label};
use super::tone;
use crate::assistant::Provider;
use crate::bridge::Action;
use crate::pages::PageCtx;
use crate::stats::{PlanUsage, Snapshot, USAGE_STALE_AFTER};
use crate::ui::canvas::{Canvas, scale_about};
use crate::ui::glass::{self, Glass};
use crate::ui::spring::{Spring, SpringConfig, UNIT_PRECISION};
use crate::ui::{color, controls, font, metrics, motion};
use layout::{Body, LIMITS, LINE_H, MESSAGE_GAP, PAD, RowRect, TITLE_H, TOP, Windows, row_rect, shown};

const PLACEHOLDER_W: f32 = 92.0;
const PLACEHOLDER_H: f32 = 8.0;
const TEXT_GAP: f32 = 12.0;
const GLYPH_SIZE: f32 = 11.0;
const GLYPH_INSET: f32 = 5.0;
const GLYPH_SPACE: f32 = 17.0;

const PRESSED_SCALE: f32 = 0.98;
const SPIN_RATE: f32 = 1.2;
const SPIN_SETTLE: SpringConfig = SpringConfig::new(0.6, 1.0);
const PENDING_FOR: Duration = Duration::from_secs(10);
const SHIMMER_PERIOD: f32 = 1.6;
const SHIMMER_STAGGER: f32 = 0.12;
const INTERACTIVE_FROM: f32 = 0.5;

pub(super) fn height(usage: &PlanUsage, assistant: Provider) -> f32 {
    Body::of(usage, assistant).height()
}

pub(super) struct UsageCard {
    bars: [Spring; LIMITS.len()],
    press: Spring,
    spin: Spring,
    pending_since: Option<Instant>,
    shimmer: f32,
    shimmering: bool,
}

impl Default for UsageCard {
    fn default() -> Self {
        Self {
            bars: std::array::from_fn(|_| Spring::new(0.0, motion::VALUE, UNIT_PRECISION)),
            press: Spring::new(1.0, motion::PRESS, UNIT_PRECISION),
            spin: Spring::new(0.0, SPIN_SETTLE, UNIT_PRECISION),
            pending_since: None,
            shimmer: 0.0,
            shimmering: false,
        }
    }
}

impl UsageCard {
    pub(super) fn is_animating(&self) -> bool {
        self.shimmering
            || self.pending_since.is_some()
            || self.bars.iter().any(|b| !b.is_settled())
            || !self.press.is_settled()
            || !self.spin.is_settled()
    }

    pub(super) fn ui(
        &mut self,
        ctx: &mut PageCtx<'_>,
        stats: &Snapshot,
        card: Rect,
        assistant: Provider,
    ) -> Option<Action> {
        let (usage, refreshing) = stats.usage(assistant);
        if self.pending_since.is_some_and(|t| !refreshing || ctx.now.duration_since(t) >= PENDING_FOR) {
            self.pending_since = None;
        }

        let id = Id::new("notchbase.stats.usage");
        let live = ctx.alpha >= INTERACTIVE_FROM;
        let response = live.then(|| ctx.ui.interact(card, id, Sense::click()));
        let hovered = response.as_ref().is_some_and(Response::hovered);
        let down = response.as_ref().is_some_and(Response::is_pointer_button_down_on);
        let clicked = response.as_ref().is_some_and(Response::clicked);
        if clicked {
            self.pending_since = Some(ctx.now);
        }
        let hover = controls::animate(ctx.ui, id.with("hover"), hovered);
        self.press.animate_to(if down { PRESSED_SCALE } else { 1.0 }, motion::PRESS);
        self.press.step(ctx.dt);
        self.spin_step(ctx.dt);
        let spinning = self.pending_since.is_some() || !self.spin.is_settled();
        let glyph = controls::animate(ctx.ui, id.with("glyph"), hovered || spinning);

        let now = SystemTime::now();
        let mut c = Canvas::new(ctx.ui.painter());
        glass::paint(&mut c, card, metrics::CARD_RADIUS, Glass::lerp(Glass::TILE, Glass::TILE_HOVER, hover));
        self.paint_header(&mut c, card, usage, assistant.title(), glyph, now);
        self.paint_body(&mut c, card, usage, assistant, ctx.dt, now);
        self.present(ctx.canvas, c, card.center());
        clicked.then_some(Action::RefreshStats)
    }

    fn spin_step(&mut self, dt: f32) {
        if self.pending_since.is_some() {
            self.spin.value += SPIN_RATE * dt;
            self.spin.velocity = SPIN_RATE;
            self.spin.target = self.spin.value.ceil();
        } else {
            self.spin.step(dt);
        }
    }

    fn paint_header(&self, c: &mut Canvas, card: Rect, usage: &PlanUsage, title: &str, glyph: f32, now: SystemTime) {
        let (left, right) = (card.left() + PAD, card.right() - PAD);
        let title_y = card.top() + TOP + TITLE_H * 0.5;
        let title = c.text(pos2(left, title_y), Align2::LEFT_CENTER, title, font::headline(), color::label());
        if glyph > 0.01 {
            let color = color::secondary_label().gamma_multiply(glyph);
            paint::refresh_glyph(c, pos2(right - GLYPH_INSET, title_y), GLYPH_SIZE, self.spin.value * TAU, color);
        }
        if let PlanUsage::Ready { fetched_at, .. } = usage {
            let age = now.duration_since(*fetched_at).unwrap_or_default();
            let text = format_ago(age);
            let color = if age > USAGE_STALE_AFTER { color::secondary_label() } else { color::tertiary_label() };
            let text_right = right - glyph * GLYPH_SPACE;
            let text = controls::truncate(c, &text, font::caption(), text_right - title.right() - TEXT_GAP);
            c.text(pos2(text_right, title_y), Align2::RIGHT_CENTER, text, font::caption(), color);
        }
    }

    fn paint_body(
        &mut self,
        c: &mut Canvas,
        card: Rect,
        usage: &PlanUsage,
        assistant: Provider,
        dt: f32,
        now: SystemTime,
    ) {
        let body = Body::of(usage, assistant);
        self.shimmering = matches!(body, Body::Placeholder);
        if self.shimmering {
            self.shimmer = (self.shimmer + dt) % (SHIMMER_PERIOD * 64.0);
        }
        match body {
            Body::Placeholder => self.paint_placeholders(c, card, dt),
            Body::Limits(windows) => self.paint_limits(c, card, &windows, assistant, dt, now),
            Body::Message(message) => self.paint_message(c, card, message),
        }
    }

    fn paint_placeholders(&mut self, c: &mut Canvas, card: Rect, dt: f32) {
        let right = card.right() - PAD;
        for (slot, label) in LIMITS.into_iter().enumerate() {
            let (row, _) = paint_label_and_track(c, card, slot, label);
            settle_bar(&mut self.bars[slot], 0.0, dt);
            let phase = ((self.shimmer - SHIMMER_STAGGER * slot as f32) / SHIMMER_PERIOD).rem_euclid(1.0);
            let placeholder = Rect::from_min_size(
                pos2(right - PLACEHOLDER_W, row.line_y - PLACEHOLDER_H * 0.5),
                vec2(PLACEHOLDER_W, PLACEHOLDER_H),
            );
            paint::placeholder(c, placeholder, phase);
            paint::shimmer_band(c, row.bar, phase);
        }
    }

    fn paint_limits(
        &mut self,
        c: &mut Canvas,
        card: Rect,
        windows: &Windows<'_>,
        assistant: Provider,
        dt: f32,
        now: SystemTime,
    ) {
        for (bar, window) in self.bars.iter_mut().zip(windows) {
            if window.is_none() {
                bar.snap_to(0.0);
            }
        }
        let right = card.right() - PAD;
        let wall = Wall::here(now);
        for (slot, (limit, window)) in shown(windows).enumerate() {
            let label = window_label(window.minutes, LIMITS[limit]);
            let (row, label_right) = paint_label_and_track(c, card, slot, &label);
            let bar = &mut self.bars[limit];
            settle_bar(bar, window.used.clamp(0.0, 1.0), dt);
            let detail = usage_detail(window, now, wall);
            let detail = controls::truncate(c, &detail, font::subheadline(), right - label_right - TEXT_GAP);
            c.text(
                pos2(right, row.line_y),
                Align2::RIGHT_CENTER,
                detail,
                font::subheadline(),
                color::secondary_label(),
            );
            let used = bar.value.clamp(0.0, 1.0);
            paint::capsule_fill(c, row.bar, used, tone::usage(assistant, used));
        }
    }

    fn paint_message(&mut self, c: &mut Canvas, card: Rect, message: &str) {
        for bar in &mut self.bars {
            bar.snap_to(0.0);
        }
        let (left, right) = (card.left() + PAD, card.right() - PAD);
        let y = card.top() + TOP + TITLE_H + MESSAGE_GAP + LINE_H * 0.5;
        let message = controls::truncate(c, message, font::callout(), right - left);
        c.text(pos2(left, y), Align2::LEFT_CENTER, message, font::callout(), color::secondary_label());
    }

    fn present(&self, canvas: &mut Canvas, card: Canvas, center: Pos2) {
        let press = self.press.value;
        let transform = scale_about(center, press);
        for mut shape in card.shapes {
            if press != 1.0 {
                shape.transform(transform);
            }
            canvas.add(shape);
        }
    }
}

fn paint_label_and_track(c: &mut Canvas, card: Rect, slot: usize, label: &str) -> (RowRect, f32) {
    let row = row_rect(card, slot);
    let label =
        c.text(pos2(card.left() + PAD, row.line_y), Align2::LEFT_CENTER, label, font::callout(), color::label());
    paint::capsule_track(c, row.bar);
    (row, label.right())
}

fn settle_bar(bar: &mut Spring, target: f32, dt: f32) {
    bar.animate_to(target, motion::VALUE);
    bar.step(dt);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settled_card_is_still() {
        assert!(!UsageCard::default().is_animating());
    }
}
