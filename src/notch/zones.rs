//! Where the notch listens: the hover, wake, click, tab and drop zones around it.

use egui::{Rect, Vec2, pos2, vec2};

use crate::config;
use crate::platform::Style;

use super::geometry::{CANVAS, Geometry};
use super::{Mode, Notch};

const HOVER_MARGIN: f32 = 8.0;
const HOVER_BELOW: f32 = 4.0;
const EXPANDED_MARGIN: f32 = 44.0;
const INPUT_MARGIN: f32 = 4.0;
const DROP_REACH: Vec2 = vec2(110.0, 90.0);
const DROP_MARGIN: f32 = 28.0;
const TAB_REACH: Vec2 = vec2(60.0, 80.0);
const TAB_MARGIN: f32 = 10.0;

impl Notch {
    pub(super) fn hover_zone(&self, g: Geometry) -> Rect {
        let b = self.bounds(g).translate(vec2(self.lean.value + self.dodge.value + self.morph.shift.value, 0.0));
        Rect::from_min_max(pos2(b.left() - HOVER_MARGIN, 0.0), pos2(b.right() + HOVER_MARGIN, b.bottom() + HOVER_BELOW))
    }

    pub(super) fn wake_zone(&self) -> Rect {
        if self.style == Style::Notch && config::current().notch.wake_at_edge {
            return Rect::from_min_size(pos2((CANVAS.x - self.notch.x) * 0.5, 0.0), self.notch);
        }
        self.hover_zone(self.rest_geometry())
    }

    pub(super) fn expanded_zone(&self) -> Rect {
        reaching_screen_top(self.bounds(self.expanded_geometry()).expand(EXPANDED_MARGIN))
    }

    pub(super) fn input_zone(&self) -> Rect {
        let silhouette = self.silhouette();
        let mut zone = silhouette.bounds();
        if let Some(sides) = self.sides_bounds(&silhouette) {
            zone = zone.union(sides);
        }
        match self.mode {
            Mode::Expanded => zone = zone.union(self.bounds(self.expanded_geometry())),
            Mode::Ask => zone = zone.union(self.bounds(self.ask_geometry())),
            _ => {}
        }
        zone.expand(INPUT_MARGIN)
    }

    pub(super) fn click_zone(&self) -> Rect {
        let zone = self.input_zone();
        if self.opens_by_click() { zone.union(self.wake_zone()) } else { zone }
    }

    pub fn tab_zone(&self) -> Rect {
        if self.mode == Mode::Drop {
            return reaching_screen_top(self.bounds(self.drop_geometry()).expand(TAB_MARGIN));
        }
        reaching_screen_top(self.bounds(self.rest_geometry()).expand2(TAB_REACH))
    }

    pub(super) fn catch_zone(&self) -> Rect {
        if self.mode == Mode::Drop {
            return reaching_screen_top(self.bounds(self.drop_geometry()).expand(DROP_MARGIN));
        }
        reaching_screen_top(self.bounds(self.rest_geometry()).expand2(DROP_REACH)).union(self.input_zone())
    }
}

fn reaching_screen_top(zone: Rect) -> Rect {
    Rect::from_min_max(pos2(zone.left(), 0.0), zone.max)
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::bridge::Page;
    use crate::notch::tests::fixtures::{AWAY, OVER, REST, draw_out, island, peek, settle, wake_anywhere_near};

    #[test]
    fn tab_zone_reaches_below() {
        let mut n = island();
        let rest = n.bounds(n.rest_geometry());
        let under = egui::pos2(rest.center().x, rest.bottom() + 70.0);
        let beside = egui::pos2(rest.right() + 100.0, rest.center().y);
        let far = egui::pos2(rest.center().x, rest.bottom() + 140.0);
        assert!(n.tab_zone().contains(under), "no need to push the tab against the top of the screen");
        assert!(n.tab_zone().contains(rest.center()) && n.tab_zone().contains(egui::pos2(rest.center().x, 0.0)));
        assert!(n.catch_zone().contains(beside) && !n.tab_zone().contains(beside), "ready, but not taken");
        assert!(!n.tab_zone().contains(far));
        n.set_drop_video(Some(crate::webvideo::Site::YouTube));
        let opened = n.bounds(n.drop_geometry());
        assert!(
            n.tab_zone().contains(egui::pos2(opened.center().x, opened.bottom() - 2.0)),
            "all it opened into counts"
        );
    }

    #[test]
    fn input_only_over_awake_shape() {
        let mut n = island();
        assert!(!n.wants_input(pos2(340.0, 3.0)), "at rest clicks go through to what is underneath");
        draw_out(&mut n, Instant::now(), OVER);
        assert!(!n.wants_input(OVER), "a pointer passing by can still click a browser tab");
        peek(&mut n, Instant::now(), OVER);
        assert!(n.wants_input(OVER));
        assert!(!n.wants_input(pos2(250.0, 200.0)));
        assert!(!n.wants_input(pos2(20.0, 20.0)));
        assert!(!n.wants_input(pos2(340.0, 200.0)));
    }

    #[test]
    fn open_panel_allows_wandering() {
        let mut n = island();
        n.open_page(Page::Tools);
        settle(&mut n);
        let panel = n.bounds(n.expanded_geometry());
        let beside = pos2(panel.right() + 30.0, panel.center().y);
        n.tick(Instant::now(), Some(beside));
        n.tick(Instant::now() + REST * 4, Some(beside));
        assert_eq!(n.mode, Mode::Expanded, "30 points away it stays open");
        let away = pos2(panel.right() + 60.0, panel.center().y);
        let start = Instant::now();
        n.tick(start, Some(away));
        n.tick(start + REST * 8, Some(away));
        assert_ne!(n.mode, Mode::Expanded, "60 points away it closes");
    }

    #[test]
    fn shorter_page_waits_for_pointer() {
        let rig = crate::notch::tests::fixtures::Rig::new();
        let keys = crate::notch::tests::fixtures::shortcuts(None);
        let mut n = island();
        n.open_page(Page::Settings);
        for _ in 0..120 {
            rig.frame(&mut n, &keys, Vec::new());
        }
        let panel = n.bounds(n.expanded_geometry());
        let at = pos2(panel.center().x, panel.bottom() - 10.0);
        let now = Instant::now();
        n.tick(now, Some(at));
        n.switch_to(Page::Tools);
        for _ in 0..120 {
            rig.frame(&mut n, &keys, Vec::new());
        }
        assert!(!n.expanded_zone().contains(at), "the panel shrank away from the pointer");
        n.tick(now, Some(at));
        n.tick(now + REST * 4, Some(at));
        assert_eq!(n.mode, Mode::Expanded, "still open where the click left the pointer");
        let back = pos2(at.x, at.y - 30.0);
        n.tick(now + REST * 5, Some(back));
        n.tick(now + REST * 9, Some(back));
        assert_eq!(n.mode, Mode::Expanded, "and on the way back up to it");
        let gone = pos2(at.x, at.y + 80.0);
        n.tick(now + REST * 10, Some(gone));
        n.tick(now + REST * 16, Some(gone));
        assert_ne!(n.mode, Mode::Expanded, "but moving on down closes it");
    }

    #[test]
    fn drops_caught_near_and_over() {
        let mut n = island();
        let rest = n.bounds(n.rest_geometry());
        assert!(n.drop_catch_zone().contains(pos2(rest.center().x, 0.0)));
        assert!(n.drop_catch_zone().contains(rest.right_bottom() + DROP_REACH * 0.9));
        assert!(!n.drop_catch_zone().contains(AWAY));

        n.pager.remeasure(std::collections::HashMap::from([(Page::Shelf, 200.0)]));
        n.open_page(Page::Shelf);
        let open = n.bounds(n.expanded_geometry());
        let low = pos2(open.center().x, open.bottom() - 2.0);
        assert!(low.y > rest.bottom() + DROP_REACH.y);
        assert!(n.drop_catch_zone().contains(low));

        n.set_drop_hover(Some(1));
        let well = n.bounds(n.drop_geometry());
        assert!(n.drop_catch_zone().contains(pos2(well.right() + DROP_MARGIN * 0.9, well.center().y)));
        assert!(!n.drop_catch_zone().contains(low));
    }

    fn rests_at(n: &mut Notch, at: egui::Pos2) -> Mode {
        let t0 = Instant::now();
        n.tick(t0, Some(AWAY));
        n.tick(t0, Some(at));
        n.tick(t0 + REST, Some(at));
        n.mode
    }

    #[test]
    fn real_notch_opens_on_itself() {
        let size = vec2(185.0, 32.0);
        let left = (CANVAS.x - size.x) * 0.5;
        let mut n = Notch::with_preview(Style::Notch, Some(size), None);
        assert_eq!(n.wake_zone(), Rect::from_min_size(pos2(left, 0.0), size));
        assert_eq!(rests_at(&mut n, pos2(left - 5.0, 10.0)), Mode::Idle, "just beside it");
        assert_eq!(rests_at(&mut n, pos2(CANVAS.x * 0.5, size.y + 3.0)), Mode::Idle, "just below it");
        assert_eq!(rests_at(&mut n, pos2(left + 4.0, size.y - 2.0)), Mode::Peek, "on it");
    }

    #[test]
    fn edge_wake_off_opens_beside() {
        let _config = wake_anywhere_near();
        let size = vec2(185.0, 32.0);
        let mut n = Notch::with_preview(Style::Notch, Some(size), None);
        assert_eq!(n.wake_zone(), n.hover_zone(n.rest_geometry()));
        assert_eq!(rests_at(&mut n, pos2((CANVAS.x - size.x) * 0.5 - 5.0, 10.0)), Mode::Peek);
    }

    #[test]
    fn island_keeps_own_wake_rule() {
        let n = island();
        assert_eq!(n.wake_zone(), n.hover_zone(n.rest_geometry()));
    }
}
