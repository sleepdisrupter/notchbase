//! Short-lived activity in the closed notch: messages, tool notices, the volume indicator and device
//! notices, popping in and fading out.

pub(super) mod device;
pub(super) mod volume;

use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::text::LayoutJob;
use egui::{Align, Galley, Painter, Pos2, Rect, Shape, Vec2, pos2, vec2};

use crate::bridge::Action;
use crate::config;
use crate::platform::Style;
use crate::ui::canvas::Canvas;
use crate::ui::color::label;
use crate::ui::element::Env;
use crate::ui::spring::{Spring, SpringConfig, UNIT_PRECISION, smoothstep};
use crate::ui::{font, motion};

use super::geometry::Geometry;
use super::moves::transition::Motion;
use super::notice::{ISLAND_H, ToolNotice};
use super::{Mode, Notch};
use device::DeviceNotice;
use volume::Indicator;

pub(super) const MESSAGE_WRAP: f32 = 304.0;
const ESTIMATED_CHAR_W: f32 = 6.4;
const ESTIMATED_LINE_H: f32 = 16.0;
pub(super) const POP_FROM: f32 = 0.4;
const POP_RELEASE: f32 = 0.2;
pub(super) const SWATCH_POP: SpringConfig = SpringConfig::new(0.40, 0.6);
pub(super) const LANE_INSET: f32 = 14.0;
const SLIDE: f32 = 8.0;
const STICKY_FOR: Duration = Duration::from_secs(60 * 60 * 24 * 365);
const JOLT: SpringConfig = SpringConfig::new(0.16, 0.16);
const JOLT_KICK: f32 = 420.0;
const SWAP_IN: SpringConfig = SpringConfig::new(0.3, 1.0);
const SWAPPED_FROM: f32 = 0.2;
const LEAVE: SpringConfig = SpringConfig::new(0.16, 1.0);
const NOD: f32 = -4.0;
const SHOWN: f32 = 0.001;

#[derive(Clone, Debug, PartialEq)]
pub(super) enum ActivityKind {
    Tool(ToolNotice),
    Message(String),
    Indicator(Indicator),
    Device(DeviceNotice),
}

impl ActivityKind {
    pub(in crate::notch) fn duration(&self) -> Duration {
        let notices = &config::current().notices;
        match self {
            Self::Tool(notice) if notice.node.sticky() => STICKY_FOR,
            Self::Tool(notice) => Duration::from_millis(notice.node.duration_ms),
            Self::Message(_) => notices.message(),
            Self::Indicator(indicator) => indicator.duration(),
            Self::Device(notice) => notices.device(notice.connected),
        }
    }

    fn same_as(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Tool(shown), Self::Tool(next)) => shown.slot == next.slot && shown.node == next.node,
            (shown, next) => shown == next,
        }
    }

    fn pops(&self) -> bool {
        match self {
            Self::Tool(notice) => notice.node.pop,
            Self::Device(notice) => notice.connected,
            Self::Message(_) | Self::Indicator(_) => false,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct Leaving {
    kind: ActivityKind,
    fade: Spring,
}

pub(super) struct Activity {
    pub(super) kind: Option<ActivityKind>,
    pub(super) until: Option<Instant>,
    pub(super) pop: Spring,
    pub(super) swap: Spring,
    pub(super) leaving: Option<Leaving>,
    pub(super) pointer: Option<Pos2>,
    message_size: Option<Vec2>,
    pop_pending: bool,
}

impl Default for Activity {
    fn default() -> Self {
        Self {
            kind: None,
            until: None,
            pop: Spring::new(1.0, motion::HOVER, UNIT_PRECISION),
            swap: Spring::new(1.0, SWAP_IN, UNIT_PRECISION),
            leaving: None,
            pointer: None,
            message_size: None,
            pop_pending: false,
        }
    }
}

impl Activity {
    pub(super) fn is_live(&self) -> bool {
        self.until.is_some()
    }

    pub(super) fn settle(&mut self) {
        self.pop.snap_to(1.0);
        self.pop_pending = false;
        self.swap.snap_to(1.0);
        self.leaving = None;
    }

    fn arm_pop(&mut self) {
        self.pop.snap_to(POP_FROM);
        self.pop_pending = true;
    }

    fn nod(&mut self) {
        if !self.pop_pending {
            self.pop.velocity = NOD;
            self.pop.animate_to(1.0, SWATCH_POP);
        }
    }

    fn release_pop(&mut self, alpha: f32) {
        if self.pop_pending && alpha > POP_RELEASE {
            self.pop_pending = false;
            self.pop.animate_to(1.0, SWATCH_POP);
        }
    }
}

pub(super) struct Lane {
    pub(super) body: Rect,
    center_y: f32,
    pub(super) slide: f32,
    lead: f32,
}

impl Lane {
    pub(super) fn leading(&self, width: f32) -> Pos2 {
        pos2(self.lead + width * 0.5 + self.slide, self.center_y)
    }

    pub(super) fn trailing(&self) -> Pos2 {
        pos2(self.body.right() - LANE_INSET - self.slide, self.center_y)
    }
}

impl Notch {
    pub(super) fn start_activity(&mut self, kind: ActivityKind, now: Instant) {
        if self.preview.is_some() {
            return;
        }
        let shown = self.activity_shown();
        let again = shown > SHOWN && self.activity.kind.as_ref().is_some_and(|current| current.same_as(&kind));
        if again {
            if kind.pops() {
                self.activity.nod();
            }
        } else {
            self.let_activity_go(shown);
            if kind.pops() {
                self.activity.arm_pop();
            }
            if matches!(kind, ActivityKind::Message(_)) {
                self.activity.message_size = None;
            }
        }
        let shake = matches!(&kind, ActivityKind::Tool(notice) if notice.node.shake);
        let pressable = matches!(&kind, ActivityKind::Tool(notice) if notice.node.controls.is_some());
        let pop = if shake { Motion::Alert } else { Motion::Pop };
        self.activity.until = Some(now + kind.duration());
        if !again {
            self.activity.kind = Some(kind);
        }
        if shake && matches!(self.mode, Mode::Idle | Mode::Activity | Mode::Peek) {
            self.jolt.velocity = if self.jolt.velocity > 0.0 { -JOLT_KICK } else { JOLT_KICK };
            self.jolt.animate_to(0.0, JOLT);
        }
        if pressable {
            self.wait_for_the_pointer_to_come_back();
        }
        match self.mode {
            Mode::Idle => self.set_mode(Mode::Activity, pop),
            Mode::Activity => self.retarget(pop),
            Mode::Peek if pressable => self.set_mode(Mode::Activity, pop),
            Mode::Peek => self.retarget(Motion::Peek),
            Mode::Expanded | Mode::Drop | Mode::Ask => {}
        }
    }

    fn let_activity_go(&mut self, shown: f32) {
        let kept = self.activity.leaving.as_ref().map_or(0.0, |leaving| self.fading_out(&leaving.fade));
        if shown <= SHOWN || kept >= shown {
            return;
        }
        let Some(kind) = self.activity.kind.clone() else { return };
        let mut fade = Spring::new(shown, LEAVE, UNIT_PRECISION);
        fade.animate_to(0.0, LEAVE);
        self.activity.leaving = Some(Leaving { kind, fade });
        self.activity.swap.snap_to(0.0);
        self.activity.swap.animate_to(1.0, SWAP_IN);
    }

    pub(super) fn step_activity_leaving(&mut self, dt: f32) {
        let Some(leaving) = &mut self.activity.leaving else { return };
        leaving.fade.step(dt);
        if leaving.fade.is_settled() {
            self.activity.leaving = None;
        }
    }

    fn activity_shown(&self) -> f32 {
        self.activity_alpha(self.morph.current()) * smoothstep(0.8, 1.0, self.attract.value)
    }

    pub(super) fn expire_activity(&mut self, now: Instant) -> bool {
        if self.activity.until.is_none_or(|until| now < until) {
            return false;
        }
        self.end_activity();
        true
    }

    pub(super) fn end_activity(&mut self) {
        self.activity.until = None;
        match self.mode {
            Mode::Activity => self.set_mode(Mode::Idle, Motion::Close),
            Mode::Peek => self.retarget(Motion::Close),
            Mode::Idle | Mode::Expanded | Mode::Drop | Mode::Ask => {}
        }
    }

    pub(super) fn shows_sticky_notice(&self) -> bool {
        self.shows_activity() && matches!(&self.activity.kind, Some(ActivityKind::Tool(notice)) if notice.node.sticky())
    }

    pub(super) fn shows_activity(&self) -> bool {
        self.activity.is_live() && matches!(self.mode, Mode::Activity | Mode::Peek)
    }

    pub(super) fn activity_geometry(&self) -> Geometry {
        let (cw, ch) = (self.notch.x, self.notch.y);
        match (self.style, &self.activity.kind) {
            (_, None) => self.idle_geometry(),
            (_, Some(ActivityKind::Indicator(_))) => self.indicator_geometry(),
            (_, Some(ActivityKind::Device(_))) => self.device_geometry(),
            (_, Some(ActivityKind::Tool(notice))) => self.notice_geometry(notice),
            (Style::Notch, Some(ActivityKind::Message(_))) => {
                let text = self.message_extent();
                Geometry { w: (cw + 56.0).max(text.x + 36.0), h: ch + text.y + 16.0, r: 16.0, ear: 6.0 }
            }
            (Style::Island, Some(ActivityKind::Message(_))) => {
                let text = self.message_extent();
                let h = if text.y > 20.0 { (text.y + 20.0).max(52.0) } else { 37.0 };
                Geometry { w: (text.x + 36.0).clamp(126.0, 340.0), h, r: h * 0.5, ear: 0.0 }
            }
        }
    }

    pub(super) fn measure_message(&mut self, painter: &Painter) {
        if self.activity.message_size.is_some() {
            return;
        }
        let Some(ActivityKind::Message(text)) = &self.activity.kind else { return };
        self.activity.message_size = Some(message_galley(painter, text).size());
        if self.shows_activity() {
            self.remeasured(if self.mode == Mode::Peek { Motion::Peek } else { Motion::Pop });
        }
    }

    pub(super) fn activity_ui(&mut self, ui: &egui::Ui, env: &Env, g: Geometry, body: Rect) -> Vec<Action> {
        if let Some(leaving) = self.activity.leaving.clone() {
            let alpha = self.fading_out(&leaving.fade);
            if alpha > SHOWN {
                self.paint_activity(ui, env, body, &leaving.kind, (alpha, false));
            }
        }
        let alpha = self.activity_alpha(g) * smoothstep(0.8, 1.0, self.attract.value);
        self.activity.release_pop(alpha);
        if alpha <= SHOWN {
            return Vec::new();
        }
        let Some(kind) = self.activity.kind.clone() else { return Vec::new() };
        self.paint_activity(ui, env, body, &kind, (alpha, true))
    }

    fn paint_activity(
        &mut self,
        ui: &egui::Ui,
        env: &Env,
        body: Rect,
        kind: &ActivityKind,
        (alpha, current): (f32, bool),
    ) -> Vec<Action> {
        let painter = ui.painter().clone();
        let lead = self.lead(body.left() + LANE_INSET);
        let lane = Lane { body, center_y: self.lane_center_y(body, kind), slide: SLIDE * (1.0 - alpha), lead };
        let pop = if current { self.activity.pop.value } else { 1.0 };
        let mut canvas = Canvas::new(&painter);
        let mut pressed = Vec::new();
        match kind {
            ActivityKind::Tool(notice) => {
                self.paint_tool_notice(ui, env, &mut canvas, &lane, notice, pop);
                let pressable = if current { alpha } else { 0.0 };
                let presses = self.paint_notice_controls(ui, env, &mut canvas, body, notice, pressable);
                if current {
                    let presses = super::notice::whats_new::pressed(notice.slot, presses);
                    pressed = super::notice::dock_intro::pressed(notice.slot, presses);
                }
            }
            ActivityKind::Message(text) => self.paint_message(&mut canvas, &painter, body, text),
            ActivityKind::Indicator(indicator) => self.paint_indicator(&mut canvas, body, *indicator),
            ActivityKind::Device(notice) => self.paint_device(&mut canvas, body, notice, pop),
        }
        let clip = painter.with_clip_rect(body.expand(1.0));
        canvas.paint(&clip, pos2(body.center().x, body.top()), 1.0, Vec2::ZERO, alpha);
        pressed
    }

    fn message_extent(&self) -> Vec2 {
        if let Some(size) = self.activity.message_size {
            return size;
        }
        let chars = match &self.activity.kind {
            Some(ActivityKind::Message(text)) => text.chars().count(),
            _ => 0,
        };
        let width = chars as f32 * ESTIMATED_CHAR_W;
        let lines = (width / MESSAGE_WRAP).ceil().max(1.0);
        vec2(width.min(MESSAGE_WRAP), lines * ESTIMATED_LINE_H)
    }

    fn activity_alpha(&self, g: Geometry) -> f32 {
        let (idle, target, open) = (self.idle_geometry(), self.activity_geometry(), self.expanded_geometry());
        let progress = |v: f32, from: f32, to: f32| if to - from > 1.0 { (v - from) / (to - from) } else { 0.0 };
        let grows = target.w - idle.w > 1.0 || target.h - idle.h > 1.0;
        let grown = if grows { progress(g.w, idle.w, target.w).max(progress(g.h, idle.h, target.h)) } else { 1.0 };
        let beyond = progress(g.h, target.h, open.h);
        let swapped = smoothstep(SWAPPED_FROM, 1.0, self.activity.swap.value);
        self.activity_fade.value.clamp(0.0, 1.0)
            * swapped
            * smoothstep(0.35, 0.9, grown)
            * (1.0 - smoothstep(0.08, 0.4, beyond))
    }

    fn lane_center_y(&self, body: Rect, kind: &ActivityKind) -> f32 {
        let stacked = matches!(kind, ActivityKind::Tool(notice) if self.stacks(notice));
        match self.style {
            Style::Notch => body.top() + self.notch.y * 0.5,
            Style::Island if stacked => body.top() + ISLAND_H * 0.5,
            Style::Island => body.center().y,
        }
    }

    fn paint_message(&self, canvas: &mut Canvas, painter: &Painter, body: Rect, text: &str) {
        let galley = message_galley(painter, text);
        let top = match self.style {
            Style::Notch => body.top() + self.notch.y + 6.0,
            Style::Island => body.center().y - galley.size().y * 0.5,
        };
        let origin = pos2(body.center().x - galley.rect.center().x, top - galley.rect.top());
        canvas.add(Shape::galley(origin, galley, label()));
    }
}

fn message_galley(painter: &Painter, text: &str) -> Arc<Galley> {
    let layout = |wrap: f32| {
        let mut job = LayoutJob::simple(text.to_owned(), font::body(), label(), wrap);
        job.halign = Align::Center;
        painter.layout_job(job)
    };
    let full = layout(MESSAGE_WRAP);
    let lines = full.rows.len();
    if lines < 2 {
        return full;
    }
    let natural = painter.layout_no_wrap(text.to_owned(), font::body(), label()).size().x;
    let balanced = layout((natural / lines as f32 + 24.0).min(MESSAGE_WRAP));
    if balanced.rows.len() == lines { balanced } else { full }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::ToolSlot;
    use crate::notch::geometry::TUCKED;
    use crate::notch::tests::fixtures::{AWAY, OVER, island, ms, sample_notice, settle};

    #[test]
    fn activity_shows_then_expires() {
        let mut n = island();
        let t0 = Instant::now();
        n.start_activity(ActivityKind::Tool(sample_notice(2_200)), t0);
        assert_eq!(n.mode, Mode::Activity);
        assert_eq!(n.attract.target, 1.0);
        assert!(!n.is_collapsed());
        assert_eq!(n.next_wakeup(), Some(t0 + ms(2_200)));
        n.tick(t0 + ms(1000), Some(AWAY));
        assert_eq!((n.mode, n.attract.target), (Mode::Activity, 1.0));
        settle(&mut n);
        assert_eq!(n.silhouette().rect(), n.body_rect(n.activity_geometry()));
        assert!(n.tick(t0 + ms(2300), Some(AWAY)));
        assert_eq!(n.mode, Mode::Idle);
        assert!(n.activity.until.is_none());
        settle(&mut n);
        n.tick(t0 + ms(2400), Some(AWAY));
        settle(&mut n);
        assert_eq!(n.silhouette().h, TUCKED.y);
        assert!(n.is_collapsed());
        assert!(!n.is_animating());
    }

    #[test]
    fn panel_closes_into_live() {
        let mut n = island();
        let t0 = Instant::now();
        n.start_activity(ActivityKind::Message("Hello".into()), t0);
        settle(&mut n);
        n.tick(t0, Some(OVER));
        n.tick(t0 + ms(260), Some(OVER));
        assert_eq!(n.mode, Mode::Peek);
        let held = t0 + crate::notch::moves::surge::SURGE;
        n.tick(held + ms(570), Some(OVER));
        assert_eq!(n.mode, Mode::Expanded);
        n.tick(held + ms(650), Some(AWAY));
        n.tick(held + ms(950), Some(AWAY));
        assert_eq!(n.mode, Mode::Activity);
        n.tick(t0 + config::current().notices.message(), Some(AWAY));
        assert_eq!(n.mode, Mode::Idle);
    }

    #[test]
    fn wide_notice_shrinks_first() {
        let mut n = island();
        let t0 = Instant::now();
        n.start_activity(ActivityKind::Tool(sample_notice(500)), t0);
        n.tick(t0, Some(AWAY));
        settle(&mut n);
        let (pill, wide) = (n.idle_geometry().w, n.morph.w.value);
        assert!(wide > pill + 30.0, "{wide}");
        n.tick(t0 + ms(600), Some(AWAY));
        assert_eq!((n.mode, n.attract.target), (Mode::Idle, 1.0), "it stays out while it is still wide");
        let mut tucked_at = None;
        for step in 0..120u64 {
            n.step(1.0 / 60.0);
            n.tick(t0 + ms(600 + step * 16), Some(AWAY));
            if n.attract.target == 0.0 {
                tucked_at = Some(n.morph.w.value);
                break;
            }
        }
        let width = tucked_at.expect("and tucks away once it is back to the pill");
        assert!(width < pill + 13.0, "{width}");
    }

    #[test]
    fn tiny_message_still_shows() {
        let mut n = island();
        let t0 = Instant::now();
        n.start_activity(ActivityKind::Message("Key copied".into()), t0);
        n.activity.message_size = Some(Vec2::new(70.0, 16.0));
        n.retarget(Motion::Pop);
        n.tick(t0, Some(AWAY));
        settle(&mut n);
        let g = n.morph.current();
        assert_eq!((g.w, g.h), (n.idle_geometry().w, n.idle_geometry().h), "it needs no more room than the capsule");
        assert!(n.activity_alpha(g) > 0.99, "and its words still show");
    }

    fn shown_notice(n: &mut Notch, t0: Instant) {
        n.start_activity(ActivityKind::Tool(sample_notice(5_000)), t0);
        n.tick(t0, Some(AWAY));
        settle(n);
        n.activity.release_pop(1.0);
        settle(n);
    }

    fn from_slot(slot: u16) -> ActivityKind {
        let mut notice = sample_notice(5_000);
        notice.slot = ToolSlot(slot);
        ActivityKind::Tool(notice)
    }

    fn seen(n: &Notch) -> f32 {
        let leaving = n.activity.leaving.as_ref().map_or(0.0, |leaving| n.fading_out(&leaving.fade));
        leaving.max(n.activity_shown())
    }

    #[test]
    fn replaced_notice_cross_fades() {
        let mut n = island();
        let t0 = Instant::now();
        shown_notice(&mut n, t0);
        n.start_activity(from_slot(3), t0 + ms(1_000));
        let leaving = n.activity.leaving.clone().expect("the first notice is still drawn while it leaves");
        assert!(matches!(&leaving.kind, ActivityKind::Tool(notice) if notice.slot == ToolSlot(0)));
        assert!(n.fading_out(&leaving.fade) > 0.99, "it leaves from where it was");
        assert!(n.activity_shown() < 0.01, "and the new one starts unseen, so its pop never jumps");
        let (mut old, mut new) = (1.0f32, 0.0f32);
        for _ in 0..30 {
            n.step(1.0 / 60.0);
            let leaving = n.activity.leaving.as_ref().map_or(0.0, |leaving| n.fading_out(&leaving.fade));
            let shown = n.activity_shown();
            assert!(leaving <= old + 1e-4 && shown >= new - 1e-4, "{leaving} {shown}");
            (old, new) = (leaving, shown);
        }
        assert!(n.activity.leaving.is_none() && new > 0.99, "{new}");
        assert!(!n.is_animating());
    }

    #[test]
    fn quick_notices_never_blink() {
        let mut n = island();
        let t0 = Instant::now();
        shown_notice(&mut n, t0);
        for (step, slot) in [3u16, 4, 5, 6].into_iter().enumerate() {
            let before = seen(&n);
            n.start_activity(from_slot(slot), t0 + ms(1_000 + step as u64 * 40));
            let after = seen(&n);
            assert!(after >= before - 0.02, "{slot}: {before} then {after}");
            for _ in 0..(1 + step * 3) {
                n.step(1.0 / 60.0);
            }
        }
        settle(&mut n);
        assert!(matches!(&n.activity.kind, Some(ActivityKind::Tool(notice)) if notice.slot == ToolSlot(6)));
        assert!(n.activity.leaving.is_none() && n.activity_shown() > 0.99);
    }

    #[test]
    fn same_notice_nods() {
        let mut n = island();
        let t0 = Instant::now();
        let mut notice = sample_notice(2_000);
        notice.node.pop = true;
        n.start_activity(ActivityKind::Tool(notice.clone()), t0);
        n.tick(t0, Some(AWAY));
        settle(&mut n);
        n.activity.release_pop(1.0);
        settle(&mut n);
        if let Some(ActivityKind::Tool(shown)) = &mut n.activity.kind {
            shown.widths = Some([20.0, 40.0]);
        }
        n.start_activity(ActivityKind::Tool(notice), t0 + ms(1_500));
        assert!(n.activity.leaving.is_none(), "nothing to cross-fade");
        let measured = matches!(&n.activity.kind, Some(ActivityKind::Tool(shown)) if shown.widths.is_some());
        assert!(measured, "it keeps its measured size");
        assert_eq!(n.activity.until, Some(t0 + ms(3_500)));
        assert_eq!(n.activity.pop.value, 1.0, "no jump to a small icon");
        let mut least = 1.0f32;
        for _ in 0..60 {
            n.step(1.0 / 60.0);
            least = least.min(n.activity.pop.value);
        }
        assert!(least < 0.95 && least > 0.8, "a small nod: {least}");
        assert!((n.activity.pop.value - 1.0).abs() < 0.01);
    }

    #[test]
    fn returning_notice_pops() {
        let mut n = island();
        let t0 = Instant::now();
        let mut notice = sample_notice(500);
        notice.node.pop = true;
        n.start_activity(ActivityKind::Tool(notice.clone()), t0);
        n.tick(t0, Some(AWAY));
        settle(&mut n);
        n.activity.release_pop(1.0);
        n.tick(t0 + ms(600), Some(AWAY));
        settle(&mut n);
        n.start_activity(ActivityKind::Tool(notice), t0 + ms(5_000));
        assert_eq!(n.activity.pop.value, POP_FROM, "a notice that already went away pops in afresh");
    }

    #[test]
    fn alert_leaves_open_panel_still() {
        let mut n = island();
        n.open_page(crate::bridge::Page::Tools);
        settle(&mut n);
        let mut notice = sample_notice(2_200);
        notice.node.shake = true;
        n.start_activity(ActivityKind::Tool(notice), Instant::now());
        assert_eq!(n.jolt.velocity, 0.0, "a shake nobody sees only knocks the panel's words out of place");
    }

    #[test]
    fn sticky_alert_shakes_and_waits() {
        let mut n = island();
        let t0 = Instant::now();
        let mut notice = sample_notice(2_200);
        notice.node.duration_ms = 0;
        notice.node.shake = true;
        n.start_activity(ActivityKind::Tool(notice), t0);
        assert_ne!(n.jolt.velocity, 0.0);
        n.tick(t0 + ms(60_000), Some(AWAY));
        assert_eq!(n.mode, Mode::Activity);
        assert!(n.shows_sticky_notice());
        n.dismiss_notice(ToolSlot(9));
        assert!(n.activity.is_live());
        n.dismiss_notice(ToolSlot(0));
        n.tick(t0 + ms(60_010), Some(AWAY));
        assert_eq!(n.mode, Mode::Idle);
        settle(&mut n);
        assert!(n.jolt.value.abs() < 0.01);
    }
}
