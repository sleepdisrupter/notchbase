//! Notices: the short messages tools post to the notch, some with buttons. Beside a camera a notice splits into
//! wings, on an island it sits in one capsule. The built-in Updated notice lives here too.

mod banner;
mod controls;
mod dismiss;
pub mod dock_intro;
#[cfg(test)]
mod tests;
pub mod whats_new;

use egui::{Painter, Rect, Vec2, pos2, vec2};
use notchbase_sdk::protocol::{NoticeNode, NoticeSize};

use crate::bridge::ToolSlot;
use crate::platform::Style;
use crate::ui::canvas::{Canvas, scale_about};
use crate::ui::element::{Env, Measure};

use super::activity::{ActivityKind, LANE_INSET, Lane, MESSAGE_WRAP};
use super::geometry::{CANVAS, Geometry};
use super::live::paint_lane;
use super::moves::transition::Motion;
use super::{Mode, Notch};
use banner::Banner;

const CAMERA_CLEARANCE: f32 = 10.0;
const ISLAND_GAP: f32 = 12.0;
pub(super) const ISLAND_H: f32 = 37.0;
const BELOW_PAD: f32 = 10.0;
const SPLIT_GAP: f32 = 8.0;
const SPLIT_EAR: f32 = 5.0;

/// A notice a tool posted, measured once so its shape can be laid out before it paints.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct ToolNotice {
    pub(super) slot: ToolSlot,
    pub(super) node: Box<NoticeNode>,
    pub(super) widths: Option<[f32; 2]>,
    pub(super) below: Vec2,
    pub(super) controls: Vec2,
    pub(super) banner: Banner,
    /// Set when the notice changed shape in place (a card opening), so the next measure moves with the open spring.
    pub(super) reshaped: bool,
}

impl ToolNotice {
    pub(super) fn new(slot: ToolSlot, node: NoticeNode) -> Self {
        Self {
            slot,
            node: Box::new(node),
            widths: None,
            below: Vec2::ZERO,
            controls: Vec2::ZERO,
            banner: Banner::default(),
            reshaped: false,
        }
    }

    fn wing(&self) -> f32 {
        match self.node.size {
            NoticeSize::Small => 64.0,
            NoticeSize::Medium => 76.0,
            NoticeSize::Large => 92.0,
        }
    }

    fn island_width(&self) -> f32 {
        match self.node.size {
            NoticeSize::Small => 176.0,
            NoticeSize::Medium | NoticeSize::Large => 236.0,
        }
    }
}

impl Notch {
    pub(super) fn notice_geometry(&self, notice: &ToolNotice) -> Geometry {
        let content = match (self.style, notice.node.banner) {
            (Style::Notch, true) => notice.banner.notch(self.notch),
            (Style::Island, true) => notice.banner.island(),
            (Style::Notch, false) => self.winged(notice),
            (Style::Island, false) => self.capsule(notice),
        };
        self.over_controls(notice, content)
    }

    fn split_wings(&self, notice: &ToolNotice) -> Option<[f32; 2]> {
        let [leading, trailing] = notice.widths.unwrap_or_default();
        let words = LANE_INSET + leading + SPLIT_GAP + trailing + CAMERA_CLEARANCE;
        let button = LANE_INSET + notice.controls.x + controls::WING_REACH + CAMERA_CLEARANCE;
        let even = notice.wing().max(words).max(button);
        let room = (2.0 * even).min(self.panel_width() - self.notch.x);
        if words + button > room {
            return None;
        }
        let left = words.max((room * 0.5).min(room - button));
        Some([left, room - left])
    }

    fn split_left_edge(&self, [left, _]: [f32; 2]) -> f32 {
        CANVAS.x * 0.5 - self.notch.x * 0.5 - left - SPLIT_EAR
    }

    pub(super) fn split_fits(&self, notice: &ToolNotice) -> bool {
        self.split_wings(notice).is_some_and(|wings| !self.menus_reach(self.split_left_edge(wings)))
    }

    pub(super) fn notice_shift(&self, notice: &ToolNotice) -> f32 {
        match self.split_wings(notice).filter(|_| self.splits(notice)) {
            Some([left, right]) => (right - left) * 0.5,
            None => 0.0,
        }
    }

    fn winged(&self, notice: &ToolNotice) -> Geometry {
        let (cw, ch) = (self.notch.x, self.notch.y);
        let [leading, trailing] = notice.widths.unwrap_or_default();
        if let Some([left, right]) = self.split_wings(notice).filter(|_| self.splits(notice)) {
            let h = ch.max(ch * 0.5 + notice.controls.y * 0.5 + controls::SPLIT_MARGIN);
            return Geometry { w: cw + left + right, h, r: 10.0, ear: SPLIT_EAR };
        }
        let wing = notice.wing().max(LANE_INSET + leading.max(trailing) + CAMERA_CLEARANCE);
        if notice.node.below.is_some() {
            let w = (cw + 2.0 * wing).max(notice.below.x + 2.0 * LANE_INSET);
            Geometry { w, h: ch + notice.below.y + 2.0 * BELOW_PAD, r: 14.0, ear: 6.0 }
        } else {
            Geometry { w: cw + 2.0 * wing, h: ch, r: 10.0, ear: 5.0 }
        }
    }

    fn capsule(&self, notice: &ToolNotice) -> Geometry {
        let [leading, trailing] = notice.widths.unwrap_or_default();
        let lane = 2.0 * LANE_INSET + leading + ISLAND_GAP + trailing + self.inline_reach(notice);
        let w = notice.island_width().max(lane).max(notice.below.x + 2.0 * LANE_INSET);
        let h = if notice.node.below.is_some() { ISLAND_H + notice.below.y + BELOW_PAD } else { ISLAND_H };
        Geometry { w, h, r: h * 0.5, ear: 0.0 }
    }

    pub(super) fn stacks(&self, notice: &ToolNotice) -> bool {
        notice.node.below.is_some() || self.controls_drop(notice) > 0.0
    }

    pub(super) fn measure_notice(&mut self, painter: &Painter, env: &Env) {
        let Some(ActivityKind::Tool(notice)) = &mut self.activity.kind else { return };
        if notice.widths.is_some() {
            return;
        }
        let canvas = Canvas::new(painter);
        let measure = Measure { canvas: &canvas, env, slot: notice.slot };
        let room = CANVAS.x;
        let widths = [measure.size(&notice.node.leading, room).x, measure.size(&notice.node.trailing, room).x];
        notice.widths = Some(widths);
        notice.below = notice.node.below.as_ref().map_or(Vec2::ZERO, |below| measure.size(below, MESSAGE_WRAP));
        notice.controls = notice.node.controls.as_ref().map_or(Vec2::ZERO, |node| measure.size(node, MESSAGE_WRAP));
        if notice.node.banner {
            notice.banner = Banner::measure(&measure, &notice.node, room);
        }
        let motion = match (std::mem::take(&mut notice.reshaped), self.mode) {
            (true, _) => Motion::Open,
            (false, Mode::Peek) => Motion::Peek,
            (false, _) => Motion::Pop,
        };
        if self.shows_activity() {
            self.remeasured(motion);
        }
    }

    pub(super) fn paint_tool_notice(
        &mut self,
        ui: &egui::Ui,
        env: &Env,
        canvas: &mut Canvas,
        lane: &Lane,
        notice: &ToolNotice,
        pop: f32,
    ) {
        if notice.node.banner {
            self.paint_banner(ui, env, canvas, lane, notice, pop);
            return;
        }
        let measure = Measure { canvas, env, slot: notice.slot };
        let room = lane.body.width();
        let leading = measure.size(&notice.node.leading, room);
        let trailing = measure.size(&notice.node.trailing, room);
        let below = notice.node.below.as_ref().map(|below| (below, measure.size(below, MESSAGE_WRAP)));
        let lead = Rect::from_center_size(lane.leading(leading.x), leading);
        let reach = self.inline_reach(notice) + trailing.x;
        let trail = if self.splits(notice) {
            Rect::from_min_size(pos2(lead.right() + SPLIT_GAP, lead.center().y - trailing.y * 0.5), trailing)
        } else {
            Rect::from_min_size(lane.trailing() - vec2(reach, trailing.y * 0.5), trailing)
        };
        let mark = canvas.mark();
        paint_lane(ui, env, canvas, &mut self.textures, notice.slot, &[(&notice.node.leading, lead)]);
        let pop = scale_about(lead.center(), pop);
        for shape in canvas.since(mark) {
            shape.transform(pop);
        }
        paint_lane(ui, env, canvas, &mut self.textures, notice.slot, &[(&notice.node.trailing, trail)]);
        if let Some((node, size)) = below {
            let top = match self.style {
                Style::Notch => lane.body.top() + self.notch.y + BELOW_PAD,
                Style::Island => lane.body.top() + ISLAND_H - BELOW_PAD * 0.5,
            };
            let rect = Rect::from_min_size(pos2(lane.body.center().x - size.x * 0.5, top), size);
            paint_lane(ui, env, canvas, &mut self.textures, notice.slot, &[(node, rect)]);
        }
    }

    fn paint_banner(
        &mut self,
        ui: &egui::Ui,
        env: &Env,
        canvas: &mut Canvas,
        lane: &Lane,
        notice: &ToolNotice,
        pop: f32,
    ) {
        let middle = match self.style {
            Style::Notch => lane.body.top() + self.notch.y + banner::NOTCH_GAP + notice.banner.size().y * 0.5,
            Style::Island => lane.body.center().y - self.controls_drop(notice) * 0.5,
        };
        let parts = notice.banner.place(pos2(lane.body.center().x, middle - lane.slide * 0.5));
        let mark = canvas.mark();
        paint_lane(ui, env, canvas, &mut self.textures, notice.slot, &[(&notice.node.leading, parts.icon)]);
        let pop = scale_about(parts.icon.center(), pop);
        for shape in canvas.since(mark) {
            shape.transform(pop);
        }
        let mut text = vec![(&notice.node.trailing, parts.title)];
        if let (Some(node), Some(rect)) = (&notice.node.below, parts.detail) {
            text.push((node, rect));
        }
        paint_lane(ui, env, canvas, &mut self.textures, notice.slot, &text);
    }
}
