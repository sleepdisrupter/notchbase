//! The ears: live activities that bud off the sides of the notch, and the web that joins them to it.

use std::f32::consts::{FRAC_PI_2, PI};

use egui::{Id, Painter, Pos2, Rect, Sense, Shape, Vec2, pos2, vec2};
use notchbase_sdk::protocol::Surface;

use crate::bridge::{Action, Activity, Page, ToolSlot, ToolView};
use crate::notch::geometry::Silhouette;
use crate::notch::moves::hug::DOCK_EAR;
use crate::notch::{Mode, Notch, keyline};
use crate::platform::Style;
use crate::ui::canvas::Canvas;
use crate::ui::element::{Env, Measure};
use crate::ui::shape::{self, Blob};
use crate::ui::spring::{Spring, SpringConfig, UNIT_PRECISION, lerp, smoothstep};

use super::{agents, paint_lane, paint_parts, video};

pub(super) const MOST: usize = 2;
const GAP: f32 = 6.0;
const LARGEST: f32 = 34.0;
const BUD: SpringConfig = SpringConfig::new(0.5, 0.58);
const LEAVE: SpringConfig = SpringConfig::new(0.32, 0.92);
const GONE: f32 = 0.01;
const BUDDING_FROM: f32 = 0.55;
const NECK_REACH: f32 = 14.0;
const SPREAD: f32 = 0.5;
const HANDLE: f32 = 2.4;
const CURVE_STEPS: usize = 20;
const WEB_REACH: f32 = 18.0;
const WEB_EAR: f32 = 3.5;
const ROUND: f32 = 0.5523;
const TUCK: f32 = 1.0;
const EAR: f32 = 4.0;
const CONTENT: f32 = 16.0;
const ABOVE: f32 = 1.0;
const HOVER: SpringConfig = SpringConfig::new(0.34, 0.72);
const HOVER_GROW: f32 = 0.16;
const HOVER_SLOP: f32 = 4.0;
const OPEN_GAP: f32 = 8.0;
const OPEN_PAD: f32 = 12.0;
const OPEN_ROOM: f32 = 220.0;
const OPENED: f32 = 0.5;
const SHOWN_FROM: f32 = 0.9;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::notch) enum Place {
    Right,
    Left,
}

impl Place {
    fn sign(self) -> f32 {
        match self {
            Place::Right => 1.0,
            Place::Left => -1.0,
        }
    }
}

pub(in crate::notch) struct Side {
    pub(in crate::notch) activity: Activity,
    pub(in crate::notch) place: Place,
    pub(in crate::notch) split: Spring,
    pub(in crate::notch) hover: Spring,
    reach: f32,
    leaving: bool,
}

struct Neck {
    outline: Vec<Pos2>,
    origin: Pos2,
    curves: Vec<Vec<Pos2>>,
}

#[derive(Clone, Copy, Debug)]
struct Bubble {
    center: Pos2,
    radius: f32,
    docked: bool,
    dock: f32,
    flare: f32,
    grown: f32,
    extra: f32,
    place: Place,
}

pub(in crate::notch) fn key(activity: &Activity) -> (u8, u64) {
    match activity {
        Activity::Tool(slot, _) => (0, u64::from(slot.0)),
        Activity::Agent(_) => (1, 0),
        Activity::Lock(_) => (2, 0),
        Activity::Video(live) => (3, live.id),
    }
}

impl Notch {
    pub(super) fn place_sides(&mut self, wanted: Vec<Activity>) {
        for side in &mut self.sides {
            side.leaving = !wanted.iter().any(|activity| key(activity) == key(&side.activity));
        }
        for activity in wanted {
            if let Some(side) = self.sides.iter_mut().find(|side| key(&side.activity) == key(&activity)) {
                side.activity = activity;
                side.leaving = false;
                continue;
            }
            let taken = |place: Place| self.sides.iter().any(|side| !side.leaving && side.place == place);
            let place = if taken(Place::Right) { Place::Left } else { Place::Right };
            self.sides.retain(|side| !(side.leaving && side.place == place));
            let (split, hover) = (Spring::new(0.0, BUD, UNIT_PRECISION), Spring::new(0.0, HOVER, UNIT_PRECISION));
            self.sides.push(Side { activity, place, split, hover, reach: 0.0, leaving: false });
        }
    }

    pub(super) fn sides_shown(&self) -> bool {
        matches!(self.mode, Mode::Idle | Mode::Peek)
            && !self.activity.is_live()
            && (self.live.is_some() || self.music_leads())
    }

    pub(in crate::notch) fn step_sides(&mut self, dt: f32) {
        let shown = self.sides_shown();
        let pinned = self.preview.is_some();
        for side in &mut self.sides {
            let (target, config) = if shown && !side.leaving { (1.0, BUD) } else { (0.0, LEAVE) };
            if side.split.target != target {
                side.split.animate_to(target, config);
            }
            if pinned {
                side.split.snap_to(target);
            }
            side.split.step(dt);
            side.hover.step(dt);
        }
        self.sides.retain(|side| !(side.leaving && side.split.value < GONE && side.split.target == 0.0));
    }

    pub(super) fn settle_sides(&mut self) {
        let shown = self.sides_shown();
        for side in &mut self.sides {
            side.split.snap_to(if shown && !side.leaving { 1.0 } else { 0.0 });
            side.hover.snap_to(side.hover.target);
        }
        self.sides.retain(|side| !side.leaving);
    }

    pub(in crate::notch) fn sides_moving(&self) -> bool {
        self.sides.iter().any(|side| !side.split.is_settled() || !side.hover.is_settled())
    }

    pub(in crate::notch) fn hover_sides(&mut self, cursor: Option<Pos2>) -> bool {
        let shown = self.sides_shown() && self.awake;
        let s = self.silhouette();
        let over: Vec<bool> = self
            .sides
            .iter()
            .map(|side| {
                let ready = shown && !side.leaving && side.split.value > SHOWN_FROM;
                ready && cursor.is_some_and(|c| bubble_rect(&self.bubble(&s, side)).expand(HOVER_SLOP).contains(c))
            })
            .collect();
        let mut changed = false;
        for (side, over) in self.sides.iter_mut().zip(over) {
            let target = if over { 1.0 } else { 0.0 };
            if side.hover.target != target {
                if over {
                    crate::platform::haptic(crate::platform::Feel::Snap);
                }
                side.hover.animate_to(target, HOVER);
                changed = true;
            }
        }
        changed
    }

    pub(in crate::notch) fn hovering_side(&self) -> bool {
        self.sides.iter().any(|side| side.hover.target > 0.0)
    }

    pub(in crate::notch) fn side_takes_input(&self, cursor: Pos2) -> bool {
        let s = self.silhouette();
        self.sides
            .iter()
            .filter(|side| side.hover.value > OPENED)
            .any(|side| bubble_rect(&self.bubble(&s, side)).expand(HOVER_SLOP).contains(cursor))
    }

    pub(in crate::notch) fn sides_bounds(&self, s: &Silhouette) -> Option<Rect> {
        self.sides
            .iter()
            .filter(|side| side.split.value > GONE)
            .map(|side| bubble_rect(&self.bubble(s, side)))
            .reduce(|a, b| a.union(b))
    }

    pub(in crate::notch) fn paint_sides_shell(&self, painter: &Painter, s: &Silhouette) {
        let pixels = painter.pixels_per_point();
        for side in self.sides.iter().filter(|side| side.split.value > GONE) {
            let bubble = self.bubble(s, side);
            if let Some(neck) = neck(s, &bubble, side.place) {
                painter.add(Shape::mesh(shape::fill_polygon(&neck.outline, neck.origin, self.body(), pixels)));
            }
            painter.add(Shape::mesh(shape::fill_with(&shell(&bubble), |_| self.body(), pixels)));
        }
    }

    pub(in crate::notch) fn paint_sides_keyline(&self, painter: &Painter, s: &Silhouette) {
        let strength = self.keyline(s);
        if strength <= keyline::SHOWN {
            return;
        }
        let (pixels, color) = (painter.pixels_per_point(), keyline::color(strength));
        for side in self.sides.iter().filter(|side| side.split.value > GONE) {
            let bubble = self.bubble(s, side);
            let outline = shape::outline(&shell(&bubble), pixels);
            painter.add(Shape::mesh(shape::band_around(&outline, keyline::WIDTH, |_| color, pixels)));
            for curve in neck(s, &bubble, side.place).iter().flat_map(|neck| &neck.curves) {
                painter.add(Shape::mesh(shape::band_along(curve, keyline::WIDTH, |_| color, pixels)));
            }
        }
    }

    pub(in crate::notch) fn sides_backdrop(&self, s: &Silhouette, pixels: f32) -> Vec<Vec<Pos2>> {
        if !crate::platform::BLURS_BEHIND {
            return Vec::new();
        }
        let shown = self.sides.iter().filter(|side| side.split.value > GONE);
        shown.map(|side| shape::outline(&shell(&self.bubble(s, side)), pixels)).collect()
    }

    pub(in crate::notch) fn sides_ui(
        &mut self,
        ui: &egui::Ui,
        env: &Env,
        s: &Silhouette,
        tools: &[ToolView],
    ) -> Vec<Action> {
        let seconds = self.clock.seconds();
        let drawn: Vec<(usize, Activity, Bubble, f32)> = self
            .sides
            .iter()
            .enumerate()
            .filter(|(_, side)| side.split.value > GONE)
            .map(|(index, side)| (index, side.activity.clone(), self.bubble(s, side), side.hover.value))
            .collect();
        let mut actions = Vec::new();
        for (index, activity, bubble, hover) in drawn {
            let alpha = smoothstep(0.35, 0.95, bubble.grown);
            if alpha <= 0.001 {
                continue;
            }
            let mut canvas = Canvas::new(ui.painter());
            let spot = Rect::from_center_size(bubble.center, Vec2::splat(CONTENT));
            match &activity {
                Activity::Agent(agent) => agents::mark(&mut canvas, agent, bubble.center, agents::MARK, &self.spin),
                Activity::Tool(slot, node) => {
                    paint_lane(ui, env, &mut canvas, &mut self.textures, *slot, &[(&node.leading, spot)]);
                }
                Activity::Video(live) => {
                    video::paint_leading(&mut canvas, live, self.video_look(live), bubble.center, seconds);
                }
                Activity::Lock(_) => {
                    super::lock::paint_padlock(&mut canvas, bubble.center, super::lock::GLYPH, self.lock_share());
                }
            }
            let scale = lerp(0.6, 1.0, bubble.grown.min(1.0)) * lerp(1.0, 1.0 + HOVER_GROW, hover.clamp(0.0, 1.0));
            canvas.paint(ui.painter(), bubble.center, scale, Vec2::ZERO, alpha);
            actions.extend(open_on_click(ui, &activity, &bubble, hover, tools));
            if self.has_tab(Page::Agents) && opens_agents(ui, &activity, &bubble, hover) {
                self.open_page(Page::Agents);
            }
            if self.has_tab(Page::Shelf) && opens_shelf(ui, &activity, &bubble, hover) {
                self.open_page(Page::Shelf);
            }
            let (reach, pressed) = self.open_ui(ui, env, &activity, &bubble, hover.clamp(0.0, 1.0) * alpha);
            self.sides[index].reach = reach;
            actions.extend(pressed);
        }
        actions
    }

    fn open_ui(
        &mut self,
        ui: &egui::Ui,
        env: &Env,
        activity: &Activity,
        bubble: &Bubble,
        open: f32,
    ) -> (f32, Vec<Action>) {
        let mut canvas = Canvas::new(ui.painter());
        let sign = bubble.place.sign();
        let y = bubble.center.y;
        let start = bubble.center.x + sign * (CONTENT * 0.5 + OPEN_GAP);
        let place = |width: f32, from: f32| {
            let left = if sign > 0.0 { from } else { from - width };
            (left, from + sign * width)
        };
        let mut actions = Vec::new();
        let reach = match activity {
            Activity::Lock(_) => CONTENT * 0.5,
            Activity::Agent(agent) => {
                let width = agents::trailing_width(&canvas, agent, env.now_ms);
                let (left, end) = place(width, start);
                if open > 0.01 {
                    agents::paint_trailing(&mut canvas, agent, pos2(left + width, y), env.now_ms);
                }
                (end - bubble.center.x).abs()
            }
            Activity::Video(live) => {
                let ear = video::Ear { live, look: self.video_look(live), start, sign, y };
                let opened = video::open(ui, &mut canvas, &ear, open > 0.01, open > OPENED);
                actions.extend(opened.action);
                (opened.end - bubble.center.x).abs()
            }
            Activity::Tool(slot, node) => {
                let measure = Measure { canvas: &canvas, env, slot: *slot };
                let trailing = measure.size(&node.trailing, OPEN_ROOM);
                let controls = node.controls.as_ref().map(|controls| (controls, measure.size(controls, OPEN_ROOM)));
                let (left, mut end) = place(trailing.x, start);
                let mut parts = vec![(&node.trailing, Rect::from_min_size(pos2(left, y - trailing.y * 0.5), trailing))];
                let mut buttons = Vec::new();
                if let Some((controls, size)) = controls {
                    let (left, after) = place(size.x, end + sign * OPEN_GAP);
                    buttons.push((controls, Rect::from_min_size(pos2(left, y - size.y * 0.5), size)));
                    end = after;
                }
                if open > 0.01 {
                    paint_parts(ui, env, &mut canvas, &mut self.textures, *slot, &parts, None);
                    let takes = (open > OPENED).then_some(Surface::Live);
                    actions = paint_parts(ui, env, &mut canvas, &mut self.textures, *slot, &buttons, takes);
                }
                parts.clear();
                (end - bubble.center.x).abs()
            }
        };
        if open > 0.01 {
            let clip = ui.painter().with_clip_rect(bubble_rect(bubble));
            canvas.paint(&clip, bubble.center, 1.0, Vec2::ZERO, smoothstep(0.3, 0.9, open));
        }
        (reach + OPEN_PAD, actions)
    }

    fn bubble(&self, s: &Silhouette, side: &Side) -> Bubble {
        let rect = s.rect();
        let grown = side.split.value.max(0.0);
        let full = s.h.min(LARGEST) * 0.5;
        let hover = side.hover.value.max(0.0);
        let radius = full * lerp(BUDDING_FROM, 1.0, grown.min(1.0)) * (1.0 + HOVER_GROW * hover);
        let (end, reach) = main_end(s, side.place);
        let travel = reach + GAP + full;
        let x = end.x + side.place.sign() * travel * grown;
        let dock = s.docking();
        let y = lerp(rect.center().y, rect.bottom() - radius, dock);
        let extra = (side.reach - radius).max(0.0) * hover;
        // The ear flares out with the main shape's own ears, so it only has them while both sit flat on
        // the edge. Flaring by `dock` alone left horns on its top corners all the way down.
        let flare = match self.style {
            Style::Island => (s.ear / DOCK_EAR).clamp(0.0, 1.0) * dock,
            Style::Notch => dock,
        };
        Bubble { center: pos2(x, y), radius, docked: dock >= 0.5, dock, flare, grown, extra, place: side.place }
    }
}

fn open_on_click(
    ui: &egui::Ui,
    activity: &Activity,
    bubble: &Bubble,
    hover: f32,
    tools: &[ToolView],
) -> Option<Action> {
    let slot = paged(activity, tools)?;
    if hover <= OPENED {
        return None;
    }
    let area = bubble_rect(bubble).expand(HOVER_SLOP);
    let response = ui.interact(area, Id::new(("notchbase.side", slot.0)), Sense::click());
    response.clicked().then_some(Action::OpenToolPage(slot))
}

fn opens_agents(ui: &egui::Ui, activity: &Activity, bubble: &Bubble, hover: f32) -> bool {
    if !matches!(activity, Activity::Agent(_)) || hover <= OPENED {
        return false;
    }
    let area = bubble_rect(bubble).expand(HOVER_SLOP);
    ui.interact(area, Id::new("notchbase.side.agents"), Sense::click()).clicked()
}

fn opens_shelf(ui: &egui::Ui, activity: &Activity, bubble: &Bubble, hover: f32) -> bool {
    let Activity::Video(live) = activity else { return false };
    let id = Id::new(("notchbase.side.video", live.id));
    if hover <= OPENED {
        return false;
    }
    let area = bubble_rect(bubble).expand(HOVER_SLOP);
    ui.interact(area, id, Sense::click()).clicked()
}

fn paged(activity: &Activity, tools: &[ToolView]) -> Option<ToolSlot> {
    let Activity::Tool(slot, _) = activity else { return None };
    tools.iter().any(|tool| tool.slot == *slot && tool.page.is_some()).then_some(*slot)
}

fn main_end(s: &Silhouette, place: Place) -> (Pos2, f32) {
    let rect = s.rect();
    let reach = s.bottom_r.clamp(1.0, s.h * 0.5);
    let x = match place {
        Place::Right => rect.right() - reach,
        Place::Left => rect.left() + reach,
    };
    let y = lerp(rect.center().y, rect.bottom() - reach, s.docking());
    (pos2(x, y), reach)
}

fn span(bubble: &Bubble) -> (f32, f32) {
    let (r, x) = (bubble.radius, bubble.center.x);
    match bubble.place {
        Place::Right => (x - r, x + r + bubble.extra),
        Place::Left => (x - r - bubble.extra, x + r),
    }
}

fn bubble_rect(bubble: &Bubble) -> Rect {
    let (left, right) = span(bubble);
    let ears = EAR * bubble.flare;
    Rect::from_min_max(pos2(left - ears, bubble_top(bubble)), pos2(right + ears, bubble.center.y + bubble.radius))
}

fn bubble_top(bubble: &Bubble) -> f32 {
    lerp(bubble.center.y - bubble.radius, 0.0, bubble.dock)
}

fn shell(bubble: &Bubble) -> Blob {
    let r = bubble.radius;
    let (left, right) = span(bubble);
    Blob {
        rect: Rect::from_min_max(pos2(left, bubble_top(bubble)), pos2(right, bubble.center.y + r)),
        top_radius: r * (1.0 - bubble.dock),
        bottom_radius: r,
        ear: EAR * bubble.grown.min(1.0) * bubble.flare,
        bleed_top: bubble.dock >= 1.0,
        wave: None,
    }
}

fn neck(s: &Silhouette, bubble: &Bubble, place: Place) -> Option<Neck> {
    if bubble.docked {
        web(s, bubble, place)
    } else {
        let (end, reach) = main_end(s, place);
        // The web has thinned to a thread by the time it hands over, so the neck grows back from one.
        let grip = smoothstep(0.5, 0.2, bubble.dock);
        metaball(end, reach, bubble.center, bubble.radius, grip)
    }
}

fn web(s: &Silhouette, bubble: &Bubble, place: Place) -> Option<Neck> {
    let rect = s.rect();
    let (main_edge, bubble_edge) = match place {
        Place::Right => (rect.right(), bubble.center.x - bubble.radius),
        Place::Left => (rect.left(), bubble.center.x + bubble.radius),
    };
    let gap = (bubble_edge - main_edge) * place.sign();
    if gap <= 0.0 {
        return None;
    }
    let apart = smoothstep(0.0, 1.0, (gap / WEB_REACH).min(1.0));
    let deepest = rect.bottom() - s.bottom_r.min(bubble.radius).max(1.0);
    // As the shapes let go of the edge the web lets go too: its top comes down off the edge and its
    // underside rises to meet it, so by the handover to the neck it's a thread between the two.
    let (end, _) = main_end(s, place);
    let thread = (end.y + bubble.center.y) * 0.5;
    let sink = ((1.0 - bubble.dock) / 0.5).clamp(0.0, 1.0);
    let roof = lerp(-ABOVE, thread, sink);
    let side = lerp(lerp(deepest, WEB_EAR, apart), thread, sink);
    let middle = lerp(lerp(deepest, 0.0, apart.powf(0.8)), thread, sink);
    let (main, ear) = (s.blob(), shell(bubble));
    let (left, right) = match place {
        Place::Right => (main, ear),
        Place::Left => (ear, main),
    };
    // Each end of the underside sits on its shape's own outline and leaves it along that
    // outline, so the web flows into a corner that rounds off early (an opened ear) instead
    // of hanging past it as a tail.
    let ((from, from_down), (to, to_down)) = (left.side_at(side, true), right.side_at(side, false));
    let top = pos2((from.x + to.x) * 0.5, middle);
    let lean = |down: Vec2| down * (ROUND * (side - middle) / down.y.max(0.2));
    let falling = cubic(to, to - lean(to_down), pos2(top.x + ROUND * (to.x - top.x), middle), top);
    let rising = cubic(top, pos2(top.x - ROUND * (top.x - from.x), middle), from - lean(from_down), from);
    let underside: Vec<Pos2> = falling.into_iter().chain(rising.into_iter().skip(1)).collect();
    // The sides tuck into the shapes they hang from, so no hairline shows where they meet.
    let tuck = |blob: &Blob| if blob.bleed_top { TUCK } else { 0.0 };
    let (left_x, right_x) = (left.rect.right() - tuck(&left), right.rect.left() + tuck(&right));
    let mut outline = vec![pos2(left_x, roof), pos2(right_x, roof)];
    let bend = right.sides_end();
    if to.y > bend && bend > roof {
        outline.push(pos2(right_x, bend));
    }
    outline.push(pos2(to.x + tuck(&right), to.y));
    outline.extend(&underside);
    outline.push(pos2(from.x - tuck(&left), from.y));
    let bend = left.sides_end();
    if from.y > bend && bend > roof {
        outline.push(pos2(left_x, bend));
    }
    outline.dedup_by(|a, b| a.distance_sq(*b) < 1e-6);
    let mid = (left.rect.right() + right.rect.left()) * 0.5;
    Some(Neck { outline, origin: pos2(mid, lerp(-ABOVE * 0.5, thread, sink)), curves: vec![underside] })
}

fn metaball(c1: Pos2, r1: f32, c2: Pos2, r2: f32, grip: f32) -> Option<Neck> {
    let d = c1.distance(c2);
    let reach = r1 + r2 + NECK_REACH;
    if d >= reach || d <= (r1 - r2).abs() + 0.5 {
        return None;
    }
    let v = SPREAD * grip * (1.0 - smoothstep(r1 + r2 - 2.0, reach, d));
    if v < 0.02 {
        return None;
    }
    let cosine = |x: f32| x.clamp(-1.0, 1.0).acos();
    let (u1, u2) = if d < r1 + r2 {
        (cosine((r1 * r1 + d * d - r2 * r2) / (2.0 * r1 * d)), cosine((r2 * r2 + d * d - r1 * r1) / (2.0 * r2 * d)))
    } else {
        (0.0, 0.0)
    };
    let between = (c2.y - c1.y).atan2(c2.x - c1.x);
    let spread = cosine((r1 - r2) / d);
    let a1 = between + u1 + (spread - u1) * v;
    let a2 = between - u1 - (spread - u1) * v;
    let a3 = between + PI - u2 - (PI - u2 - spread) * v;
    let a4 = between - PI + u2 + (PI - u2 - spread) * v;
    let at = |center: Pos2, radius: f32, angle: f32| center + vec2(angle.cos(), angle.sin()) * radius;
    let (p1, p2, p3, p4) = (at(c1, r1, a1), at(c1, r1, a2), at(c2, r2, a3), at(c2, r2, a4));
    let total = r1 + r2;
    let handle = (v * HANDLE).min(p1.distance(p3) / total) * (d * 2.0 / total).min(1.0);
    let h1 = at(p1, r1 * handle, a1 - FRAC_PI_2);
    let h2 = at(p2, r1 * handle, a2 + FRAC_PI_2);
    let h3 = at(p3, r2 * handle, a3 + FRAC_PI_2);
    let h4 = at(p4, r2 * handle, a4 - FRAC_PI_2);
    let mut curves = vec![cubic(p1, h1, h3, p3), cubic(p4, h4, h2, p2)];
    if shape::signed_area(&curves.concat()) < 0.0 {
        curves.reverse();
        curves.iter_mut().for_each(|curve| curve.reverse());
    }
    Some(Neck { outline: curves.concat(), origin: c1.lerp(c2, 0.5), curves })
}

fn cubic(a: Pos2, b: Pos2, c: Pos2, d: Pos2) -> Vec<Pos2> {
    (0..=CURVE_STEPS)
        .map(|step| {
            let t = step as f32 / CURVE_STEPS as f32;
            let u = 1.0 - t;
            let weights = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
            let point = [a, b, c, d].iter().zip(weights).fold(Vec2::ZERO, |sum, (p, w)| sum + p.to_vec2() * w);
            point.to_pos2()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::bridge::Page;
    use crate::notch::live::tests::{claude, recording};
    use crate::notch::preview::Preview;
    use crate::notch::tests::fixtures::{AWAY, OVER, REST, Rig, island, ms, settle, shortcuts, song, tool_view};
    use crate::notch::tests::snapshot::SCALE;

    fn timing(slot: u16) -> Activity {
        let Activity::Tool(_, node) = recording(0) else { unreachable!() };
        Activity::Tool(ToolSlot(slot), node)
    }

    fn click(rig: &Rig, n: &mut Notch, at: Pos2) -> Vec<Action> {
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        rig.frame(n, &[], vec![egui::Event::PointerMoved(at)]);
        rig.frame(n, &[], vec![button(true)]);
        rig.frame(n, &[], vec![button(false)])
    }

    fn opened_ear(n: &mut Notch) -> Pos2 {
        let t0 = Instant::now();
        n.tick(t0, Some(AWAY));
        settle(n);
        let ear = n.bubble(&n.silhouette(), &n.sides[0]).center;
        n.tick(t0, Some(pos2(ear.x, 0.0)));
        settle(n);
        let ear = n.bubble(&n.silhouette(), &n.sides[0]).center;
        n.tick(t0, Some(ear));
        settle(n);
        n.tick(t0 + REST * 2, Some(ear));
        settle(n);
        ear
    }

    #[test]
    fn ear_click_opens_tool_page() {
        let mut rig = Rig::new();
        rig.tools.push(tool_view::<timer::Timer>(3));
        let mut n = island();
        n.set_activities(vec![claude(), timing(3)]);
        let ear = opened_ear(&mut n);
        assert!(n.wants_input(ear));
        let actions = click(&rig, &mut n, ear);
        assert!(actions.iter().any(|action| matches!(action, Action::OpenToolPage(ToolSlot(3)))));
        assert_ne!(n.mode, Mode::Expanded, "the ear takes the click instead of the shape behind it");
        n.open_page(Page::Tool(ToolSlot(3)));
        assert_eq!(n.mode, Mode::Expanded);
    }

    #[test]
    fn pageless_ear_opens_notch() {
        let rig = Rig::new();
        let mut n = island();
        n.set_activities(vec![claude(), timing(2)]);
        let ear = opened_ear(&mut n);
        let actions = click(&rig, &mut n, ear);
        assert!(!actions.iter().any(|action| matches!(action, Action::OpenToolPage(_))));
        assert_eq!(n.mode, Mode::Expanded);
        assert_eq!(paged(&claude(), &rig.tools), None);
    }

    #[test]
    fn song_leaving_glides() {
        let mut n = island();
        let t0 = Instant::now();
        n.set_activities(vec![claude()]);
        n.set_now_playing(Some(&song(true, 10.0)));
        n.tick(t0, Some(AWAY));
        settle(&mut n);
        let wide = n.silhouette().w;
        n.expire_music(t0 + crate::config::current().music.reveal() + ms(100));
        let mut last = wide;
        let mut steepest: f32 = 0.0;
        for frame in 0..120 {
            if frame == 1 {
                n.fit_live([16.0, 60.0]);
            }
            n.step(1.0 / 120.0);
            let w = n.silhouette().w;
            steepest = steepest.max((w - last).abs());
            last = w;
        }
        assert!(wide - last > 30.0, "it narrowed from {wide} to {last}");
        assert!(steepest < 4.0, "the widest single-frame step was {steepest}");
    }

    #[test]
    fn hovered_ear_grows() {
        let mut n = island();
        let t0 = Instant::now();
        n.set_activities(vec![claude(), recording(0)]);
        n.tick(t0, Some(AWAY));
        settle(&mut n);
        let s = n.silhouette();
        let resting = n.bubble(&s, &n.sides[0]);
        assert!(!n.wants_input(resting.center));
        n.tick(t0, Some(resting.center));
        settle(&mut n);
        assert_eq!(n.sides[0].hover.target, 0.0, "a pointer on a browser tab under the ear leaves it alone");
        n.tick(t0, Some(pos2(resting.center.x, 0.0)));
        settle(&mut n);
        let resting = n.bubble(&n.silhouette(), &n.sides[0]);
        n.tick(t0, Some(resting.center));
        settle(&mut n);
        n.tick(t0 + REST * 2, Some(resting.center));
        let s = n.silhouette();
        let open = n.bubble(&s, &n.sides[0]);
        assert!(open.radius > resting.radius * 1.1);
        assert!(n.wants_input(resting.center));
        assert_eq!(n.mode, Mode::Idle, "resting on an ear doesn't peek the main activity");
        n.sides[0].reach = 120.0;
        settle(&mut n);
        let open = n.bubble(&n.silhouette(), &n.sides[0]);
        let far_end = pos2(bubble_rect(&open).right() - 2.0, open.center.y);
        for step in 0..30 {
            n.tick(t0 + REST * 2 + ms(step * 16), Some(far_end));
            n.step(1.0 / 60.0);
        }
        assert_eq!(n.sides[0].hover.target, 1.0, "the far end of an opened ear still counts as the ear");
        assert!(n.hug.target < 0.5, "the main shape stays put while the ear is open");
        n.tick(t0 + REST * 3, Some(AWAY));
        settle(&mut n);
        assert!(!n.wants_input(resting.center));
    }

    #[test]
    fn slide_onto_ear_opens_ear() {
        let mut n = island();
        n.set_activities(vec![claude(), recording(0)]);
        let t0 = crate::notch::tests::fixtures::peek(&mut n, Instant::now(), OVER);
        assert_eq!(n.mode, Mode::Peek);
        settle(&mut n);
        let ear = n.bubble(&n.silhouette(), &n.sides[0]).center;
        for step in 0..30 {
            n.tick(t0 + ms(step * 16), Some(ear));
            n.step(1.0 / 60.0);
        }
        assert!(n.awake, "leaving the main shape for an ear keeps the notch awake");
        assert_ne!(n.mode, Mode::Expanded);
        assert_eq!(n.sides[0].hover.target, 1.0, "the ear opens under the pointer");
    }

    #[test]
    fn neck_thins_and_lets_go() {
        let main = pos2(100.0, 20.0);
        let near = metaball(main, 17.0, pos2(100.0 + 17.0 + 4.0 + 17.0, 20.0), 17.0, 1.0).expect("a neck when close");
        let far = metaball(main, 17.0, pos2(100.0 + 34.0 + 10.0, 20.0), 17.0, 1.0).expect("a thinner neck further out");
        let thickness = |points: &[Pos2]| {
            let middle = points.iter().filter(|p| (p.x - 121.0).abs() < 3.0).map(|p| p.y);
            let (low, high) = middle.fold((f32::MAX, f32::MIN), |(lo, hi), y| (lo.min(y), hi.max(y)));
            high - low
        };
        assert!(thickness(&near.outline) > thickness(&far.outline));
        assert!(metaball(main, 17.0, pos2(100.0 + 34.0 + NECK_REACH + 1.0, 20.0), 17.0, 1.0).is_none());
        assert!(shape::signed_area(&near.outline) > 0.0);
    }

    #[test]
    fn docked_tabs_share_web() {
        let s = Silhouette {
            center_x: 200.0,
            top: 0.0,
            w: 120.0,
            h: 24.0,
            top_r: 0.0,
            bottom_r: 12.0,
            ear: 4.0,
            bleed_top: true,
            wave: None,
        };
        let bubble = |gap: f32| Bubble {
            center: pos2(260.0 + gap + 12.0, 12.0),
            radius: 12.0,
            docked: true,
            dock: 1.0,
            flare: 1.0,
            grown: 1.0,
            extra: 0.0,
            place: Place::Right,
        };
        let crest = |gap: f32| {
            let underside = web(&s, &bubble(gap), Place::Right).unwrap().curves.remove(0);
            underside.iter().map(|p| p.y).fold(f32::MAX, f32::min)
        };
        assert!(crest(2.0) > crest(12.0), "a closer tab hangs a deeper web");
        assert!(web(&s, &bubble(-3.0), Place::Right).is_none());
    }

    fn notch_beside_claude() -> Silhouette {
        Silhouette {
            center_x: 340.0,
            top: 0.0,
            w: 297.0,
            h: 32.0,
            top_r: 0.0,
            bottom_r: 10.0,
            ear: 5.0,
            bleed_top: true,
            wave: None,
        }
    }

    fn ear_beside(s: &Silhouette, place: Place, radius: f32, gap: f32, extra: f32) -> Bubble {
        let edge = if place == Place::Right { s.rect().right() } else { s.rect().left() };
        Bubble {
            center: pos2(edge + place.sign() * (gap + radius), s.rect().bottom() - radius),
            radius,
            docked: true,
            dock: 1.0,
            flare: 1.0,
            grown: 1.0,
            extra,
            place,
        }
    }

    fn inside(outline: &[Pos2], p: Pos2) -> bool {
        let mut inside = false;
        for (i, a) in outline.iter().enumerate() {
            let b = outline[(i + 1) % outline.len()];
            if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x) {
                inside = !inside;
            }
        }
        inside
    }

    fn off(outline: &[Pos2], p: Pos2) -> f32 {
        let span = |i: usize| (outline[i], outline[(i + 1) % outline.len()]);
        let nearest = |(a, b): (Pos2, Pos2)| {
            let t = ((p - a).dot(b - a) / (b - a).length_sq().max(1e-9)).clamp(0.0, 1.0);
            p.distance(a + (b - a) * t)
        };
        (0..outline.len()).map(|i| nearest(span(i))).fold(f32::MAX, f32::min)
    }

    #[test]
    fn web_meets_outlines() {
        let s = notch_beside_claude();
        let main = shape::outline(&s.blob(), SCALE);
        let resting = (16.0, 6.0, 0.0);
        let opened = (16.0 * (1.0 + HOVER_GROW), 3.44, 98.0);
        let opening = (17.0, 5.0, 8.0);
        let budding = (14.56, 1.0, 0.0);
        for place in [Place::Right, Place::Left] {
            for (radius, gap, extra) in [resting, opened, opening, budding] {
                let bubble = ear_beside(&s, place, radius, gap, extra);
                let ear = shape::outline(&shell(&bubble), SCALE);
                let web = web(&s, &bubble, place).expect("a web across the gap");
                let underside = &web.curves[0];
                for end in [underside[0], underside[underside.len() - 1]] {
                    let on = off(&main, end).min(off(&ear, end));
                    assert!(on < 0.05, "{place:?} ear of {radius}: the web ends {on} away from both outlines");
                }
                let covered = |outline: &[Pos2], p: Pos2| inside(outline, p) || off(outline, p) < 0.02;
                for &corner in web.outline.iter().filter(|p| !underside.contains(p)) {
                    let hidden = covered(&main, corner) || covered(&ear, corner);
                    assert!(hidden, "{place:?} ear of {radius}: the web pokes out at {corner:?}");
                }
            }
        }
    }

    #[test]
    fn web_hides_the_seam() {
        let s = notch_beside_claude();
        for place in [Place::Right, Place::Left] {
            let bubble = ear_beside(&s, place, 16.0, 6.0, 0.0);
            let web = web(&s, &bubble, place).unwrap();
            let underside = &web.curves[0];
            let shallowest = underside[0].y.min(underside[underside.len() - 1].y);
            let main_edge = if place == Place::Right { s.rect().right() } else { s.rect().left() };
            let ear_edge = bubble.center.x - place.sign() * bubble.radius;
            for step in 0..=20 {
                let y = lerp(-0.5, shallowest - 0.5, step as f32 / 20.0);
                for (edge, into) in [(main_edge, -place.sign()), (ear_edge, place.sign())] {
                    let tucked = pos2(edge + into * TUCK * 0.5, y);
                    assert!(inside(&web.outline, tucked), "{place:?}: the web stops short of the shape at {tucked:?}");
                }
            }
        }
    }

    #[test]
    fn ear_stretches_into_tab() {
        let n = island();
        let side = Side {
            activity: Activity::Lock(false),
            place: Place::Right,
            split: Spring::new(1.0, BUD, UNIT_PRECISION),
            hover: Spring::new(0.0, HOVER, UNIT_PRECISION),
            reach: 0.0,
            leaving: false,
        };
        let floating = Silhouette {
            center_x: 340.0,
            top: 10.0,
            w: 200.0,
            h: 37.0,
            top_r: 18.5,
            bottom_r: 18.5,
            ear: 0.0,
            bleed_top: false,
            wave: None,
        };
        let mut last: Option<(f32, f32, f32)> = None;
        for step in 0..=40 {
            let lift = step as f32 / 40.0;
            let s = Silhouette {
                top: lerp(10.0, 0.0, lift),
                top_r: lerp(18.5, 0.0, lift),
                bleed_top: lift >= 1.0,
                ..floating
            };
            let blob = shell(&n.bubble(&s, &side));
            let now = (blob.rect.top(), blob.top_radius, blob.ear);
            if let Some(before) = last {
                let moved = [(now.0 - before.0).abs(), (now.1 - before.1).abs(), (now.2 - before.2).abs()];
                assert!(moved.iter().all(|step| *step < 1.5), "lift {lift}: {before:?} to {now:?}");
            }
            last = Some(now);
        }
        assert_eq!(last.map(|(top, radius, _)| (top, radius)), Some((0.0, 0.0)), "it ends as a tab on the edge");
    }

    #[test]
    fn ear_lets_go_of_the_edge_cleanly() {
        let n = island();
        let side = Side {
            activity: Activity::Lock(false),
            place: Place::Right,
            split: Spring::new(1.0, BUD, UNIT_PRECISION),
            hover: Spring::new(0.0, HOVER, UNIT_PRECISION),
            reach: 0.0,
            leaving: false,
        };
        let docked = Silhouette {
            center_x: 340.0,
            top: 0.0,
            w: 116.0,
            h: 24.0,
            top_r: 0.0,
            bottom_r: 12.0,
            ear: DOCK_EAR,
            bleed_top: true,
            wave: None,
        };
        assert!(shell(&n.bubble(&docked, &side)).ear > 0.0, "flat on the edge, the ear flares like the shape");
        let mut thinnest = f32::MAX;
        for step in 1..=40 {
            let lift = step as f32 / 40.0;
            let s = Silhouette {
                top: lerp(0.0, 10.0, lift),
                h: lerp(24.0, 37.0, lift),
                top_r: lerp(0.0, 18.5, lift),
                ear: 0.0,
                bleed_top: false,
                ..docked
            };
            let bubble = n.bubble(&s, &side);
            assert_eq!(shell(&bubble).ear, 0.0, "lift {lift}: no horns once the shape has left the edge");
            if let Some(neck) = neck(&s, &bubble, side.place).filter(|_| bubble.docked) {
                let roof = neck.outline.iter().map(|p| p.y).fold(f32::MAX, f32::min);
                assert!(roof >= -ABOVE && (lift < 0.05 || roof > 0.0), "lift {lift}: the web comes off the edge too");
                let floor = neck.outline.iter().map(|p| p.y).fold(f32::MIN, f32::max);
                thinnest = thinnest.min(floor - roof);
            }
        }
        assert!(thinnest < 1.0, "the web is a thread by the time the neck takes over, not {thinnest}");
    }

    #[test]
    fn keyline_skips_the_neck() {
        let mut n = Notch::with_preview(Style::Island, None, Some(Preview::Sides));
        let image = Rig::new().shoot(&mut n, &shortcuts(None), 30);
        let pixel = |x: f32, y: f32| image.get_pixel((x * SCALE) as u32, (y * SCALE) as u32).0;
        let keyline = |x: f32, y: f32| pixel(x, y).iter().all(|&channel| channel.abs_diff(29) <= 2);
        let (s, half) = (n.silhouette(), keyline::WIDTH * 0.5);
        assert_eq!(n.sides.len(), 2);
        for side in &n.sides {
            let (bubble, sign) = (n.bubble(&s, side), side.place.sign());
            let end = s.rect().center().x + sign * s.rect().width() * 0.5;
            let (near, far) = (bubble.center.x - sign * bubble.radius, bubble.center.x + sign * bubble.radius);
            let y = bubble.center.y;
            assert_eq!(pixel(end + sign * half, y), [0, 0, 0], "the neck leaves the main shape with no line across it");
            assert_eq!(pixel(near - sign * half, y), [0, 0, 0], "and meets the ear with none either");
            assert!(keyline(far + sign * half, y), "the keyline rounds the far end of the ear");
            assert!(keyline(bubble.center.x, y + bubble.radius + half), "and runs under it");
            let waist = (end + near) * 0.5;
            let edge = (0..80).map(|step| y + step as f32 * 0.5).find(|&below| pixel(waist, below) != [0, 0, 0]);
            assert!(edge.is_some_and(|edge| edge > y && keyline(waist, edge + 0.5)), "and under the neck's waist");
        }
    }
}
