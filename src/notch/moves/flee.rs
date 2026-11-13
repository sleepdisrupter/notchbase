//! Holding Alt makes the notch run from the pointer, and letting go brings it home.

use egui::{Pos2, Rect, vec2};

use crate::notch::geometry::CANVAS;
use crate::notch::{Mode, Notch};
use crate::ui::spring::{SpringConfig, smoothstep};

use super::attract::distance_to_rect;

pub(in crate::notch) const FLEE: SpringConfig = SpringConfig::new(0.26, 0.5);
pub(in crate::notch) const FEAR: SpringConfig = SpringConfig::new(0.18, 1.0);
const HOMEWARD: SpringConfig = SpringConfig::new(0.4, 0.62);
const RANGE: f32 = 96.0;
const NEAR: f32 = 24.0;
const MARGIN: f32 = 30.0;
const STILL: f32 = 0.5;

impl Notch {
    pub fn fear(&mut self, keys: bool, pressing: bool, cursor: Option<Pos2>) -> bool {
        self.pressed_near = match (pressing, self.pressed_near) {
            (false, _) => None,
            (true, Some(near)) => Some(near),
            (true, None) => Some(cursor.is_some_and(|c| self.frightens(c) && !self.wants_input(c))),
        };
        self.set_scared(keys || self.pressed_near == Some(true))
    }

    fn frightens(&self, cursor: Pos2) -> bool {
        distance_to_rect(self.home(), cursor) < RANGE
    }

    pub(super) fn set_scared(&mut self, scared: bool) -> bool {
        if scared == self.scared || self.preview.is_some() {
            return false;
        }
        self.scared = scared;
        if !scared && self.flee.target != 0.0 {
            self.hover_armed = false;
            self.awake = false;
        }
        true
    }

    pub(in crate::notch) fn shies(&self) -> bool {
        self.scared && matches!(self.mode, Mode::Idle | Mode::Peek | Mode::Activity)
    }

    pub(in crate::notch) fn flee_from(&mut self, cursor: Option<Pos2>) -> bool {
        let (fear, push) = cursor.filter(|_| self.shies()).map_or((0.0, 0.0), |c| self.push_from(c));
        if fear > 0.0 {
            self.feared = cursor;
        }
        let frightened = (self.fear.target - fear).abs() > 1e-3;
        self.fear.animate_to(fear, FEAR);
        if (self.flee.target - push).abs() <= STILL {
            return frightened;
        }
        self.flee.animate_to(push, if push != 0.0 { FLEE } else { HOMEWARD });
        true
    }

    fn push_from(&self, cursor: Pos2) -> (f32, f32) {
        let home = self.home();
        let strength = smoothstep(RANGE, NEAR, distance_to_rect(home, cursor));
        if strength <= 0.0 {
            return (0.0, 0.0);
        }
        let right = (cursor.x + MARGIN - home.left()).max(0.0);
        let left = (cursor.x - MARGIN - home.right()).min(0.0);
        let (clears_right, clears_left) = (right <= CANVAS.x - home.right(), -left <= home.left());
        let prefers_right = match self.flee.target {
            t if t != 0.0 => t > 0.0,
            _ => cursor.x < home.center().x,
        };
        let goes_right = if prefers_right { clears_right || !clears_left } else { clears_right && !clears_left };
        let push = if goes_right { right.min(CANVAS.x - home.right()) } else { left.max(-home.left()) };
        (strength, strength * push)
    }

    fn home(&self) -> Rect {
        let s = self.silhouette();
        let shown = self.sides_bounds(&s).map_or(s.rect(), |sides| sides.union(s.rect()));
        shown.translate(vec2(-self.flee.value, 0.0))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use egui::pos2;

    use super::*;
    use crate::notch::moves::transition::Motion;
    use crate::notch::tests::fixtures::{AWAY, REST, island, ms, peek, settle, song};

    fn docked_song() -> Notch {
        let mut n = island();
        n.set_now_playing(Some(&song(true, 10.0)));
        n.tick(Instant::now(), Some(AWAY));
        settle(&mut n);
        n
    }

    fn hold(n: &mut Notch, at: Pos2) {
        for _ in 0..4 {
            n.tick(Instant::now(), Some(at));
            settle(n);
        }
    }

    #[test]
    fn alt_pushes_it_away() {
        let mut n = docked_song();
        let tab = pos2(CANVAS.x * 0.5 + 20.0, 14.0);
        assert!(n.silhouette().rect().contains(tab));
        n.set_scared(true);
        hold(&mut n, tab);
        let rect = n.silhouette().rect();
        assert!(
            rect.right() <= tab.x - MARGIN + 0.5,
            "it slides clear of the pointer, away from the side it came from"
        );
        assert!(rect.left() >= 0.0);
        assert_eq!(n.mode, Mode::Idle);
        assert!(!n.awake && !n.wants_input(tab), "so the click lands on the tab under it");
    }

    #[test]
    fn springs_away_like_magnet() {
        let mut n = docked_song();
        n.set_scared(true);
        let tab = pos2(CANVAS.x * 0.5 - 10.0, 14.0);
        n.tick(Instant::now(), Some(tab));
        let target = n.flee.target;
        assert!(target > 0.0);
        let mut furthest: f32 = 0.0;
        for _ in 0..120 {
            n.step(1.0 / 120.0);
            furthest = furthest.max(n.flee.value);
        }
        assert!(furthest > target, "it overshoots a little before it settles");
    }

    #[test]
    fn far_pointer_leaves_it() {
        let mut n = docked_song();
        n.set_scared(true);
        hold(&mut n, AWAY);
        hold(&mut n, pos2(CANVAS.x * 0.5, 120.0));
        assert_eq!(n.flee.value, 0.0);
    }

    #[test]
    fn keeps_running_one_way() {
        let mut n = docked_song();
        n.set_scared(true);
        let center = CANVAS.x * 0.5;
        hold(&mut n, pos2(center - 10.0, 14.0));
        assert!(n.flee.target > 0.0);
        hold(&mut n, pos2(center + 10.0, 14.0));
        assert!(n.flee.target > 0.0, "crossing where it used to sit doesn't make it jump back through the pointer");
        assert!(!n.silhouette().rect().contains(pos2(center + 10.0, 14.0)));
    }

    #[test]
    fn letting_go_brings_it_home() {
        let mut n = docked_song();
        let tab = pos2(CANVAS.x * 0.5 + 20.0, 14.0);
        n.set_scared(true);
        hold(&mut n, tab);
        n.set_scared(false);
        let t0 = Instant::now();
        n.tick(t0, Some(tab));
        settle(&mut n);
        assert_eq!(n.flee.value, 0.0);
        n.tick(t0 + REST, Some(tab));
        n.tick(t0 + REST * 3, Some(tab));
        assert_eq!(n.mode, Mode::Idle, "it waits for the pointer to leave before it peeks again");
    }

    #[test]
    fn peek_backs_off_on_alt() {
        let mut n = island();
        let t0 = peek(&mut n, Instant::now(), pos2(340.0, 28.0));
        assert_eq!(n.mode, Mode::Peek);
        n.set_scared(true);
        n.tick(t0 + ms(20), Some(pos2(340.0, 28.0)));
        assert_eq!(n.mode, Mode::Idle);
        assert!(n.flee.target != 0.0);
    }

    #[test]
    fn open_panel_ignores_alt() {
        let mut n = island();
        n.set_mode(Mode::Expanded, Motion::Open);
        n.set_scared(true);
        let inside = pos2(340.0, 120.0);
        n.tick(Instant::now(), Some(inside));
        n.tick(Instant::now() + ms(400), Some(inside));
        assert_eq!((n.mode, n.flee.target), (Mode::Expanded, 0.0));
    }

    #[test]
    fn nearby_click_scares_it() {
        let mut n = docked_song();
        let tab = pos2(CANVAS.x * 0.5 + 20.0, 14.0);
        n.fear(false, true, Some(tab));
        hold(&mut n, tab);
        assert!(n.scared && !n.silhouette().rect().contains(tab));
        n.fear(false, false, Some(tab));
        assert!(!n.scared, "letting go of the button calms it");
    }

    #[test]
    fn far_drag_can_still_drop() {
        let mut n = docked_song();
        n.fear(false, true, Some(AWAY));
        let over = pos2(CANVAS.x * 0.5, 14.0);
        n.fear(false, true, Some(over));
        hold(&mut n, over);
        assert!(!n.scared && n.flee.value == 0.0, "a file dragged in from elsewhere isn't a scare");
    }

    #[test]
    fn pressing_on_it_never_scares() {
        let mut n = island();
        let t0 = peek(&mut n, Instant::now(), pos2(340.0, 28.0));
        assert!(n.wants_input(pos2(340.0, 28.0)));
        n.fear(false, true, Some(pos2(340.0, 28.0)));
        n.tick(t0 + ms(20), Some(pos2(340.0, 28.0)));
        assert!(!n.scared);
        assert_eq!(n.mode, Mode::Peek);
    }
}
