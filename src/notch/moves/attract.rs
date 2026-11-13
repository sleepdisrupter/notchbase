//! Leaning toward a pointer that comes near, and waking when it reaches the edge.

use egui::{Pos2, Rect, pos2};

use crate::config;
use crate::notch::geometry::{CANVAS, TUCKED};
use crate::notch::{Mode, Notch};
use crate::platform::Style;
use crate::ui::spring::{SpringConfig, smoothstep};

pub(in crate::notch) const ATTRACT: SpringConfig = SpringConfig::new(0.28, 0.9);
pub(in crate::notch) const LEAN: SpringConfig = SpringConfig::new(0.35, 0.8);
const ATTRACT_FAR: f32 = 24.0;
const ATTRACT_NEAR: f32 = 6.0;
const AWAKE_FAR: f32 = 70.0;
const WAKE_NEAR: f32 = 16.0;
const WAKE_BAND: f32 = 7.0;
const CLOSED_REACH: f32 = 8.0;
const TUCKED_AWAY: f32 = 0.05;
const DOCKED: f32 = 0.95;
const LEAN_GAIN: f32 = 0.22;
const LEAN_MAX: f32 = 28.0;
const CAPSULE_GROWTH: f32 = 12.0;
pub(super) const EDGE_BAND: f32 = 4.0;

pub(in crate::notch) fn resting_attract(style: Style) -> f32 {
    if style == Style::Notch { 1.0 } else { 0.0 }
}

impl Notch {
    pub(in crate::notch) fn attract_toward(&mut self, cursor: Option<Pos2>) -> bool {
        let distance = cursor.map(|c| distance_to_rect(self.presence(), c));
        self.wake_toward(cursor, distance);
        let hugged = self.hug_toward(cursor);
        if self.style != Style::Island {
            return hugged;
        }
        let near = cursor.map_or(0.0, |c| self.reach(c));
        let attract = if self.needs_capsule() { 1.0 } else { near };
        let lean = match (self.mode, cursor) {
            (Mode::Expanded, _) | (_, None) => 0.0,
            (_, Some(c)) => ((c.x - CANVAS.x * 0.5) * LEAN_GAIN).clamp(-LEAN_MAX, LEAN_MAX) * near,
        };
        let moved = (self.attract.target - attract).abs() > 1e-3 || (self.lean.target - lean).abs() > 0.05;
        self.attract.animate_to(attract, ATTRACT);
        self.lean.animate_to(lean, LEAN);
        moved || hugged
    }

    pub(in crate::notch) fn reach(&self, cursor: Pos2) -> f32 {
        match self.awake {
            false => 0.0,
            true if self.hovering_side() => 1.0,
            true => proximity(distance_to_rect(self.presence(), cursor)),
        }
    }

    fn presence(&self) -> Rect {
        let s = self.silhouette();
        let tab = Rect::from_min_size(pos2(CANVAS.x * 0.5 - TUCKED.x * 0.5, 0.0), TUCKED);
        let mut shown = tab.union(s.rect());
        if self.hugs_top() {
            shown = shown.union(self.docked_rect());
        }
        let shown = self.sides_bounds(&s).map_or(shown, |sides| shown.union(sides));
        if self.fully_closed() { shown.expand(CLOSED_REACH) } else { shown }
    }

    fn fully_closed(&self) -> bool {
        self.mode == Mode::Idle && (self.attract.value < TUCKED_AWAY || self.hug.value > DOCKED)
    }

    fn wake_toward(&mut self, cursor: Option<Pos2>, distance: Option<f32>) {
        let edge_only = self.style == Style::Island && config::current().notch.wake_at_edge;
        self.awake = match (cursor, distance) {
            _ if !edge_only => true,
            (Some(c), Some(d)) if d < AWAKE_FAR => {
                self.awake || self.mode != Mode::Idle || c.y <= EDGE_BAND || (c.y <= WAKE_BAND && d <= WAKE_NEAR)
            }
            _ => self.mode != Mode::Idle,
        };
    }

    pub(in crate::notch) fn needs_capsule(&self) -> bool {
        self.pinned
            || self.activity.is_live()
            || self.live.is_some()
            || self.music_revealed()
            || matches!(self.mode, Mode::Peek | Mode::Expanded | Mode::Drop | Mode::Ask)
            || self.morph.h.value > self.idle_geometry().h + CAPSULE_GROWTH
            || (self.floats() && self.morph.w.value > self.idle_geometry().w + CAPSULE_GROWTH)
    }
}

impl Notch {
    fn floats(&self) -> bool {
        self.hug.target == 0.0 && self.hug.is_settled()
    }
}

pub(super) fn proximity(distance: f32) -> f32 {
    smoothstep(ATTRACT_FAR, ATTRACT_NEAR, distance)
}

pub(super) fn distance_to_rect(rect: Rect, p: Pos2) -> f32 {
    let dx = (rect.left() - p.x).max(p.x - rect.right()).max(0.0);
    let dy = (rect.top() - p.y).max(p.y - rect.bottom()).max(0.0);
    dx.hypot(dy)
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::notch::moves::transition::Motion;
    use crate::notch::tests::fixtures::{draw_out, island, settle};

    #[test]
    fn proximity_falls_with_distance() {
        assert_eq!(proximity(500.0), 0.0);
        assert_eq!(proximity(ATTRACT_FAR), 0.0);
        assert_eq!(proximity(ATTRACT_NEAR), 1.0);
        assert_eq!(proximity(0.0), 1.0);
        let mut last = 1.0;
        for d in 0..=120 {
            let p = proximity(d as f32);
            assert!(p <= last + 1e-6, "not monotonic at {d}");
            last = p;
        }
        let tab = Rect::from_min_size(pos2(298.0, 0.0), TUCKED);
        assert_eq!(distance_to_rect(tab, pos2(340.0, 3.0)), 0.0);
        assert_eq!(distance_to_rect(tab, pos2(340.0, 35.0)), 30.0);
        assert_eq!(distance_to_rect(tab, pos2(292.0, 13.0)), 10.0);
    }

    #[test]
    fn tab_strip_crossing_ignored() {
        let mut n = island();
        let t0 = Instant::now();
        for x in (240..=440).step_by(20) {
            n.tick(t0, Some(pos2(x as f32, 36.0)));
        }
        assert!(!n.awake);
        assert_eq!(n.attract.target, 0.0, "a pointer on a browser tab a little below doesn't draw it out");
        settle(&mut n);
        assert!(!n.wants_input(pos2(340.0, 36.0)));
    }

    #[test]
    fn closed_island_reaches_further() {
        let far = 340.0 + TUCKED.x * 0.5 + WAKE_NEAR + CLOSED_REACH - 2.0;
        let mut n = island();
        n.tick(Instant::now(), Some(pos2(far, WAKE_BAND - 1.0)));
        assert!(n.awake, "the tucked tab reaches out a little further along the edge");
        let mut n = island();
        draw_out(&mut n, Instant::now(), pos2(340.0, 30.0));
        assert!(!n.fully_closed());
        assert!(
            n.presence().bottom() <= n.silhouette().rect().bottom() + 0.01,
            "once it's out, the extra reach is gone"
        );
    }

    #[test]
    fn tab_button_never_wakes() {
        let mut n = island();
        let t0 = Instant::now();
        for y in (WAKE_BAND as u32 + 1)..=30 {
            for x in [300.0, 340.0, 380.0, 400.0] {
                n.tick(t0, Some(pos2(x, y as f32)));
            }
        }
        assert!(!n.awake, "only a pointer above a tab's buttons wakes it");
        assert_eq!(n.attract.target, 0.0);
    }

    #[test]
    fn edge_approach_draws_out() {
        let mut n = island();
        let t0 = Instant::now();
        let close = pos2(340.0 + TUCKED.x * 0.5 + WAKE_NEAR - 2.0, WAKE_BAND - 1.0);
        for _ in 0..90 {
            n.tick(t0, Some(close));
            n.step(1.0 / 60.0);
        }
        assert!(n.awake);
        assert_eq!(n.attract.target, 1.0, "right next to it, it comes out fully");
        assert!(n.silhouette().rect().height() > TUCKED.y * 3.0, "it grows out toward the pointer");
    }

    #[test]
    fn edge_push_wakes() {
        let mut n = island();
        let t0 = Instant::now();
        n.tick(t0, Some(pos2(350.0, 0.0)));
        assert!(n.awake && n.attract.target > 0.0);
        settle(&mut n);
        n.tick(t0, Some(pos2(350.0, 30.0)));
        assert!(n.awake && n.attract.target > 0.0, "it stays awake while you come down to it");
        n.tick(t0, Some(pos2(40.0, 300.0)));
        assert!(!n.awake);
        settle(&mut n);
        n.tick(t0, Some(pos2(350.0, 34.0)));
        assert!(!n.awake, "hovering a little below doesn't wake it again");
        assert_eq!(n.attract.target, 0.0);
    }

    #[test]
    fn leans_but_opens_centered() {
        let mut n = island();
        let t0 = Instant::now();
        n.tick(t0, Some(pos2(380.0, 0.0)));
        settle(&mut n);
        n.tick(t0, Some(pos2(380.0, 20.0)));
        assert!(n.lean.target > 0.0 && n.lean.target <= LEAN_MAX);
        n.tick(t0, Some(pos2(40.0, 20.0)));
        assert_eq!(n.lean.target, 0.0, "too far away to lean");
        draw_out(&mut n, t0, pos2(380.0, 20.0));
        n.set_mode(Mode::Expanded, Motion::Open);
        n.tick(t0, Some(pos2(380.0, 20.0)));
        assert_eq!(n.lean.target, 0.0);
        settle(&mut n);
        assert_eq!(n.silhouette().center_x, CANVAS.x * 0.5);
    }
}
