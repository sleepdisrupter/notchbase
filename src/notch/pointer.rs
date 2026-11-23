//! What the pointer does to the notch: a peek on hover, opening after a dwell, closing once it leaves.

use std::time::{Duration, Instant};

use egui::{Pos2, Rect};

use crate::config;

use super::moves::surge::SURGE;
use super::moves::transition::Motion;
use super::{Mode, Notch};

const DRAWN_OUT: f32 = 0.95;
const PEEK_AFTER: Duration = Duration::from_millis(150);
pub(super) const STILL: f32 = 12.0;
const SWITCH_SLACK: f32 = 44.0;

impl Notch {
    pub(super) fn follow_pointer(&mut self, now: Instant, cursor: Option<Pos2>) -> bool {
        let inside = |zone: Rect| cursor.is_some_and(|c| zone.contains(c));
        if self.mode != Mode::Expanded {
            self.switched_at = None;
        }
        match self.mode {
            Mode::Idle | Mode::Activity => {
                let hovering = inside(self.wake_zone())
                    && !self.hovering_side()
                    && !self.hovers_controls(cursor)
                    && !self.hovers_live_controls(cursor);
                self.peek_when_hovered(now, cursor, hovering)
            }
            Mode::Peek if self.hovers_controls(cursor) || self.hovers_live_controls(cursor) => {
                self.return_to_rest();
                true
            }
            Mode::Peek => {
                let hovering = inside(self.hover_zone(self.peek_of(self.rest_geometry()))) && !self.hovering_side();
                self.open_after_dwell(now, cursor, hovering)
            }
            Mode::Expanded => {
                let near = inside(self.expanded_zone());
                let returning = self.returning_after_switch(cursor, near).is_some_and(inside);
                let held = self.pinned || self.pages.holds_open(self.pager.page) || near || returning;
                self.close_after_leaving(now, held)
            }
            Mode::Drop | Mode::Ask => false,
        }
    }

    fn returning_after_switch(&mut self, cursor: Option<Pos2>, near: bool) -> Option<Rect> {
        if self.switched_from != self.pager.page {
            self.switched_from = self.pager.page;
            self.switched_at = cursor.filter(|_| !near);
        }
        if near {
            self.switched_at = None;
        }
        let at = self.switched_at?;
        let zone = self.expanded_zone();
        Some(Rect::from_min_max(zone.min, egui::pos2(zone.right(), zone.bottom().max(at.y + SWITCH_SLACK))))
    }

    fn peek_when_hovered(&mut self, now: Instant, cursor: Option<Pos2>, hovering: bool) -> bool {
        let wanted = hovering && self.attract.value > DRAWN_OUT && config::current().notch.open_on_hover;
        if !self.hover_armed {
            self.hover_armed = !hovering;
            self.peek_until = None;
            return false;
        }
        match (wanted, self.peek_until) {
            (false, _) => {
                self.peek_until = None;
                false
            }
            (true, None) => {
                self.peek_until = Some(now + PEEK_AFTER);
                false
            }
            (true, Some(at)) if now < at => false,
            (true, Some(_)) => {
                self.peek_until = None;
                self.set_mode(Mode::Peek, Motion::Peek);
                self.hold_still_from(now, cursor);
                true
            }
        }
    }

    pub(super) fn opens_by_click(&self) -> bool {
        self.mode == Mode::Idle && !config::current().notch.open_on_hover
    }

    pub(super) fn awaits_click(&self, cursor: Pos2) -> bool {
        self.opens_by_click()
            && self.hover_armed
            && !self.shies()
            && !self.hovering_side()
            && self.attract.value > DRAWN_OUT
            && self.wake_zone().contains(cursor)
    }

    fn hold_still_from(&mut self, now: Instant, cursor: Option<Pos2>) {
        self.still_at = cursor;
        self.dwell_until = Some(now + config::current().notch.hover_delay() + SURGE);
    }

    fn restless(&self, cursor: Option<Pos2>) -> bool {
        matches!((self.still_at, cursor), (Some(at), Some(c)) if at.distance(c) > STILL)
    }

    fn open_after_dwell(&mut self, now: Instant, cursor: Option<Pos2>, hovering: bool) -> bool {
        if !hovering {
            if !self.hovering_side() {
                self.awake = false;
            }
            self.return_to_rest();
            true
        } else if self.restless(cursor) {
            self.hold_still_from(now, cursor);
            true
        } else if self.dwell_until.is_none_or(|t| now >= t) {
            self.opens_to_start();
            self.set_mode(Mode::Expanded, Motion::Open);
            true
        } else {
            false
        }
    }

    fn close_after_leaving(&mut self, now: Instant, held: bool) -> bool {
        if held {
            self.leave_until = None;
            return false;
        }
        match self.leave_until {
            None => {
                self.leave_until = Some(now + config::current().notch.close_delay());
                false
            }
            Some(t) if now >= t => {
                self.awake = false;
                self.return_to_rest();
                true
            }
            Some(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use egui::pos2;

    use crate::bridge::Page;
    use crate::notch::activity::ActivityKind;
    use crate::notch::geometry::TUCKED;
    use crate::notch::tests::fixtures::{
        AWAY, OVER, REST, Rig, click, draw_out, hover_open_off, island, ms, peek, sample_notice, settle,
    };

    use super::*;

    const DWELL: Duration = Duration::from_millis(120 + SURGE.as_millis() as u64);
    const LEAVE_GRACE: Duration = Duration::from_millis(220);

    #[test]
    fn hover_peeks_then_opens_after_dwell() {
        let mut n = island();
        let t0 = Instant::now();
        assert!(n.tick(t0, Some(pos2(OVER.x, 0.0))));
        settle(&mut n);
        n.tick(t0, Some(OVER));
        assert_eq!(n.mode, Mode::Idle);
        settle(&mut n);
        n.tick(t0, Some(OVER));
        assert_eq!(n.mode, Mode::Idle, "passing over it isn't enough");
        assert_eq!(n.next_wakeup(), Some(t0 + PEEK_AFTER));
        let t0 = t0 + REST;
        assert!(n.tick(t0, Some(OVER)));
        assert_eq!(n.mode, Mode::Peek);
        assert_eq!(n.next_wakeup(), Some(t0 + DWELL));
        n.tick(t0 + ms(200), Some(OVER));
        assert_eq!(n.mode, Mode::Peek);
        n.tick(t0 + DWELL - ms(10), Some(pos2(OVER.x + STILL * 0.6, OVER.y)));
        assert_eq!(n.mode, Mode::Peek, "a little wobble inside the deadzone is still holding still");
        assert!(n.tick(t0 + DWELL + ms(10), Some(OVER)));
        assert_eq!(n.mode, Mode::Expanded);
        assert!(n.is_animating());
    }

    #[test]
    fn mission_control_keeps_it_closed() {
        let mut n = island();
        n.set_mission_control(true);
        let t0 = Instant::now();
        for step in 0..20u64 {
            n.tick(t0 + ms(step * 100), Some(OVER));
        }
        assert_eq!(n.mode, Mode::Idle, "resting on it over Mission Control never peeks or opens");
        assert!(!n.wants_input(OVER), "and clicks go through to Mission Control");

        n.set_mission_control(false);
        let t1 = t0 + ms(2_000);
        n.tick(t1, Some(OVER));
        n.tick(t1 + REST, Some(OVER));
        assert_eq!(n.mode, Mode::Idle, "back from it, the pointer still on the notch has to leave first");
        n.tick(t1 + REST + ms(10), Some(AWAY));
        settle(&mut n);
        peek(&mut n, t1 + ms(1_000), OVER);
        assert_eq!(n.mode, Mode::Peek);
    }

    #[test]
    fn mission_control_closes_an_open_panel() {
        let mut n = island();
        let t0 = peek(&mut n, Instant::now(), OVER) + SURGE;
        n.tick(t0 + ms(300), Some(OVER));
        assert_eq!(n.mode, Mode::Expanded);
        n.set_mission_control(true);
        assert_eq!(n.mode, Mode::Idle);
    }

    #[test]
    fn restless_pointer_never_opens() {
        let mut n = island();
        let t0 = peek(&mut n, Instant::now(), OVER);
        assert_eq!(n.mode, Mode::Peek);
        for step in 0..40u64 {
            let x = OVER.x + if step % 2 == 0 { -STILL * 1.5 } else { STILL * 1.5 };
            n.tick(t0 + ms(step * 50), Some(pos2(x, OVER.y)));
        }
        assert_eq!(n.mode, Mode::Peek, "two seconds of frantic moving keeps it a peek");
        let rested = t0 + ms(40 * 50);
        n.tick(rested + DWELL + ms(10), Some(pos2(OVER.x + STILL * 1.5, OVER.y)));
        assert_eq!(n.mode, Mode::Expanded, "and it opens once the pointer settles");
    }

    #[test]
    fn slam_to_top_edge_counts() {
        let mut n = island();
        peek(&mut n, Instant::now(), pos2(340.0, 0.0));
        assert_eq!(n.mode, Mode::Peek);
    }

    #[test]
    fn leaving_peek_goes_idle() {
        let mut n = island();
        let t0 = peek(&mut n, Instant::now(), OVER);
        assert_eq!(n.mode, Mode::Peek);
        assert!(n.tick(t0 + ms(100), Some(AWAY)));
        assert_eq!(n.mode, Mode::Idle);
        assert_eq!(n.attract.target, 0.0);
        settle(&mut n);
        assert_eq!(n.silhouette().rect(), Rect::from_min_size(pos2(340.0 - 42.0, 0.0), TUCKED));
        assert!(n.is_collapsed());
    }

    #[test]
    fn open_panel_waits_leave_grace() {
        let mut n = island();
        let t0 = peek(&mut n, Instant::now(), OVER) + SURGE;
        n.tick(t0 + ms(300), Some(OVER));
        assert_eq!(n.mode, Mode::Expanded);

        let inside = pos2(340.0, 120.0);
        let t1 = t0 + ms(400);
        n.tick(t1, Some(inside));
        assert_eq!(n.mode, Mode::Expanded);

        n.tick(t1 + ms(10), Some(AWAY));
        n.tick(t1 + ms(150), Some(AWAY));
        assert_eq!(n.mode, Mode::Expanded);
        n.tick(t1 + ms(200), Some(inside));
        n.tick(t1 + ms(600), Some(AWAY));
        assert_eq!(n.mode, Mode::Expanded);
        assert_eq!(n.next_wakeup(), Some(t1 + ms(600) + LEAVE_GRACE));
        assert!(n.tick(t1 + ms(830), Some(AWAY)));
        assert_eq!(n.mode, Mode::Idle);
    }

    #[test]
    fn unknown_cursor_counts_as_outside() {
        let mut n = island();
        let t0 = Instant::now();
        n.set_mode(Mode::Expanded, Motion::Open);
        n.tick(t0, None);
        n.tick(t0 + ms(250), None);
        assert_eq!(n.mode, Mode::Idle);
    }

    #[test]
    fn pinned_panel_ignores_leaving() {
        let mut n = island();
        let t0 = Instant::now();
        n.set_pinned(true);
        assert_eq!(n.mode, Mode::Expanded);
        n.tick(t0, Some(AWAY));
        n.tick(t0 + ms(1000), Some(AWAY));
        assert_eq!(n.mode, Mode::Expanded);
        n.set_pinned(false);
        n.tick(t0 + ms(1010), Some(AWAY));
        n.tick(t0 + ms(1300), Some(AWAY));
        assert_eq!(n.mode, Mode::Idle);
    }

    #[test]
    fn tool_settings_stay_open() {
        let mut n = island();
        let t0 = Instant::now();
        n.set_mode(Mode::Expanded, Motion::Open);
        n.open_tool_settings("calendar");
        n.tick(t0, Some(AWAY));
        n.tick(t0 + ms(1000), Some(AWAY));
        assert_eq!(n.mode, Mode::Expanded);
        n.open_page(Page::Tools);
        n.tick(t0 + ms(1010), Some(AWAY));
        n.tick(t0 + ms(1300), Some(AWAY));
        assert_eq!(n.mode, Mode::Idle, "on another page it closes as usual");
    }

    #[test]
    fn collapse_holds_until_leave() {
        let mut n = island();
        let t0 = Instant::now();
        n.start_activity(ActivityKind::Tool(sample_notice(2_200)), t0);
        n.set_mode(Mode::Expanded, Motion::Open);
        n.collapse();
        assert_eq!(n.mode, Mode::Idle);
        assert!(n.activity.until.is_none());
        assert_eq!(n.next_wakeup(), None);

        draw_out(&mut n, t0 + ms(10), OVER);
        n.tick(t0 + ms(400), Some(OVER));
        assert_eq!(n.mode, Mode::Idle);
        n.tick(t0 + ms(500), Some(AWAY));
        peek(&mut n, t0 + ms(600), OVER);
        assert_eq!(n.mode, Mode::Peek);
    }

    #[test]
    fn sweeping_past_never_opens() {
        let mut n = island();
        let t0 = Instant::now();
        draw_out(&mut n, t0, OVER);
        for step in 0..5 {
            n.tick(t0 + ms(step * 40), Some(OVER));
        }
        n.tick(t0 + ms(220), Some(AWAY));
        n.tick(t0 + ms(600), Some(AWAY));
        assert_eq!(n.mode, Mode::Idle);
    }

    #[test]
    fn click_mode_rest_never_peeks() {
        let _config = hover_open_off();
        let mut n = island();
        let t0 = Instant::now();
        draw_out(&mut n, t0, OVER);
        for step in 0..20 {
            n.tick(t0 + REST + ms(step * 100), Some(OVER));
        }
        assert_eq!(n.mode, Mode::Idle);
        assert_eq!(n.next_wakeup(), None, "no peek and no dwell are waiting");
        assert!(n.wants_input(OVER), "but the island takes the click");
        assert!(!n.wants_input(AWAY));
    }

    #[test]
    fn click_mode_click_opens() {
        let _config = hover_open_off();
        let rig = Rig::new();
        let mut n = island();
        draw_out(&mut n, Instant::now(), OVER);
        click(&rig, &mut n, OVER);
        assert_eq!(n.mode, Mode::Expanded);
        assert!(n.is_animating());
    }

    #[test]
    fn click_mode_zone_takes_click() {
        let _config = hover_open_off();
        let rig = Rig::new();
        let mut n = island();
        draw_out(&mut n, Instant::now(), OVER);
        let zone = n.hover_zone(n.rest_geometry());
        let corner = zone.left_top() + egui::vec2(1.0, 1.0);
        assert!(n.wants_input(corner));
        click(&rig, &mut n, corner);
        assert_eq!(n.mode, Mode::Expanded, "every point that swallows a click also opens it");
    }

    #[test]
    fn click_mode_leaving_returns_input() {
        let _config = hover_open_off();
        let mut n = island();
        let t0 = Instant::now();
        draw_out(&mut n, t0, OVER);
        assert!(n.wants_input(OVER));
        n.tick(t0 + ms(400), Some(AWAY));
        assert_eq!(n.mode, Mode::Idle);
        assert!(!n.wants_input(AWAY));
        settle(&mut n);
        assert!(!n.wants_input(OVER), "tucked away again it lets clicks through");
        assert!(n.is_collapsed());
        assert_eq!(n.next_wakeup(), None);
    }

    #[test]
    fn click_mode_collapse_waits() {
        let _config = hover_open_off();
        let mut n = island();
        let t0 = Instant::now();
        n.set_mode(Mode::Expanded, Motion::Open);
        n.collapse();
        draw_out(&mut n, t0, OVER);
        assert!(!n.wants_input(OVER));
        n.tick(t0 + ms(100), Some(AWAY));
        draw_out(&mut n, t0 + ms(200), OVER);
        assert!(n.wants_input(OVER));
    }

    #[test]
    fn drawn_out_lets_clicks_through() {
        let mut n = island();
        draw_out(&mut n, Instant::now(), OVER);
        assert!(!n.wants_input(OVER));
        assert_eq!(n.click_zone(), n.input_zone());
    }
}
