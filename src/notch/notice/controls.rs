//! A notice's buttons: where they sit (inline, in a wing or underneath), what counts as hovering, and painting them.

use std::time::{Duration, Instant};

use egui::{Pos2, Rect, Vec2, pos2};
use notchbase_sdk::protocol::{Node, Surface};

use crate::bridge::Action;
use crate::notch::moves::transition::Motion;
use crate::platform::Style;
use crate::ui::canvas::Canvas;
use crate::ui::element::Env;

use super::super::activity::{ActivityKind, LANE_INSET};
use super::super::geometry::Geometry;
use super::super::live::paint_parts;
use super::super::{Mode, Notch};
use super::{ISLAND_H, ToolNotice};

pub(super) const INLINE_GAP: f32 = 10.0;
const INLINE_INSET_MIN: f32 = 5.0;
const WING_INSET: f32 = 8.0;
pub(super) const WING_REACH: f32 = WING_INSET - super::super::activity::LANE_INSET;
pub(super) const SPLIT_MARGIN: f32 = INLINE_INSET_MIN;
const UNDER_GAP: f32 = 8.0;
const UNDER_PAD: f32 = 10.0;
const UNDER_RADIUS: f32 = 14.0;
const UNDER_EAR: f32 = 6.0;
const ISLAND_RADIUS_MAX: f32 = 30.0;
pub(super) const CONTROLS_REACH: f32 = 6.0;
pub(super) const HELD_FOR: Duration = Duration::from_secs(1);
const PRESSABLE_FROM: f32 = 0.6;

impl Notch {
    fn tucks_controls_inline(&self, notice: &ToolNotice) -> bool {
        let plain = !notice.node.banner && notice.node.below.is_none();
        match self.style {
            Style::Island => plain && notice.controls.y + 2.0 * INLINE_INSET_MIN <= ISLAND_H,
            Style::Notch => plain && self.splits(notice),
        }
    }

    pub(super) fn splits(&self, notice: &ToolNotice) -> bool {
        self.style == Style::Notch
            && notice.node.controls.as_ref().is_some_and(|node| one_button(node, self.style))
            && notice.controls.y > 0.0
            && notice.controls.y <= self.notch.y
            && !notice.node.banner
            && notice.node.below.is_none()
            && self.split_fits(notice)
    }

    pub(in crate::notch) fn follow_split(&mut self) {
        let split = match &self.activity.kind {
            Some(ActivityKind::Tool(notice)) => self.splits(notice),
            _ => false,
        };
        if split == self.split_shown {
            return;
        }
        self.split_shown = split;
        if self.shows_activity() && matches!(self.mode, Mode::Activity | Mode::Peek) {
            self.remeasured(Motion::Resize);
        }
    }

    pub(super) fn inline_reach(&self, notice: &ToolNotice) -> f32 {
        if notice.controls.x <= 0.0 || self.style != Style::Island || !self.tucks_controls_inline(notice) {
            return 0.0;
        }
        notice.controls.x + INLINE_GAP + inline_inset(notice) - LANE_INSET
    }

    pub(super) fn controls_drop(&self, notice: &ToolNotice) -> f32 {
        if notice.controls.y <= 0.0 || self.tucks_controls_inline(notice) {
            return 0.0;
        }
        let padded = notice.node.banner || notice.node.below.is_some();
        UNDER_GAP + notice.controls.y + if padded { 0.0 } else { UNDER_PAD }
    }

    pub(super) fn over_controls(&self, notice: &ToolNotice, content: Geometry) -> Geometry {
        let drop = self.controls_drop(notice);
        if drop <= 0.0 {
            return content;
        }
        let (w, h) = (content.w.max(notice.controls.x + 2.0 * LANE_INSET), content.h + drop);
        match self.style {
            Style::Notch => Geometry { w, h, r: content.r.max(UNDER_RADIUS), ear: content.ear.max(UNDER_EAR) },
            Style::Island => Geometry { w, h, r: (h * 0.5).min(ISLAND_RADIUS_MAX), ear: 0.0 },
        }
    }

    pub(super) fn controls_rect(&self, body: Rect, notice: &ToolNotice) -> Rect {
        let size = notice.controls;
        if self.splits(notice) {
            let (right, center_y) = (body.right() - WING_INSET, body.top() + self.notch.y * 0.5);
            return Rect::from_min_size(pos2(right - size.x, center_y - size.y * 0.5), size);
        }
        if self.tucks_controls_inline(notice) {
            let right = body.right() - inline_inset(notice);
            return Rect::from_min_size(pos2(right - size.x, body.center().y - size.y * 0.5), size);
        }
        let bottom = body.top() + self.notice_geometry(notice).h - UNDER_PAD;
        Rect::from_min_size(pos2(body.center().x - size.x * 0.5, bottom - size.y), size)
    }

    pub(super) fn controls_zone(&self) -> Option<Rect> {
        let Some(ActivityKind::Tool(notice)) = &self.activity.kind else { return None };
        if !self.shows_activity() || notice.controls == Vec2::ZERO {
            return None;
        }
        Some(self.controls_rect(self.silhouette().rect(), notice).expand(CONTROLS_REACH))
    }

    pub(in crate::notch) fn hovers_controls(&self, calm: Option<Pos2>) -> bool {
        calm.is_some_and(|at| self.controls_zone().is_some_and(|zone| zone.contains(at)))
    }

    pub(super) fn pointer_is_on_controls(&self) -> bool {
        self.hovers_controls(self.activity.pointer)
    }

    pub(in crate::notch) fn hold_hovered_notice(&mut self, now: Instant, cursor: Option<Pos2>) {
        self.activity.pointer = cursor.filter(|_| !self.shies());
        if self.pointer_is_on_controls() {
            self.activity.until = self.activity.until.map(|until| until.max(now + HELD_FOR));
        }
    }

    pub(in crate::notch) fn wait_for_the_pointer_to_come_back(&mut self) {
        self.hover_armed = false;
        self.peek_until = None;
    }

    pub(in crate::notch) fn paint_notice_controls(
        &mut self,
        ui: &egui::Ui,
        env: &Env,
        canvas: &mut Canvas,
        body: Rect,
        notice: &ToolNotice,
        alpha: f32,
    ) -> Vec<Action> {
        let Some(node) = &notice.node.controls else { return Vec::new() };
        let rect = self.controls_rect(body, notice);
        let pressable = self.shows_activity() && self.preview.is_none() && !self.locked && alpha > PRESSABLE_FROM;
        let takes = pressable.then_some(Surface::Notice);
        paint_parts(ui, env, canvas, &mut self.textures, notice.slot, &[(node, rect)], takes)
    }
}

fn one_button(node: &Node, style: Style) -> bool {
    match (node, style) {
        (Node::Button { .. }, _) => true,
        (Node::Adaptive { notch, .. }, Style::Notch) => one_button(notch, style),
        (Node::Adaptive { island, .. }, Style::Island) => one_button(island, style),
        _ => false,
    }
}

fn inline_inset(notice: &ToolNotice) -> f32 {
    (ISLAND_H - notice.controls.y) * 0.5
}
