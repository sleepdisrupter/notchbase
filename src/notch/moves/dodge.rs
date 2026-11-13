//! Sliding aside when the menu bar's menus reach the notch.

use crate::notch::geometry::CANVAS;
use crate::notch::{Mode, Notch};
use crate::platform::Style;
use crate::ui::spring::SpringConfig;

pub(in crate::notch) const SLIDE: SpringConfig = SpringConfig::new(0.4, 0.92);
const MENU_GAP: f32 = 8.0;
const CAMERA_CLEARANCE: f32 = 10.0;
const RESETTLE: f32 = 0.5;

impl Notch {
    pub fn set_menu_edge(&mut self, edge: Option<f32>) {
        self.menu_edge = edge;
    }

    pub(super) fn follow_dodge(&mut self) {
        let target = self.dodge_target();
        if (self.dodge.target - target).abs() <= RESETTLE {
            return;
        }
        let crowded = (self.dodge.target > 0.0) != (target > 0.0);
        self.dodge.animate_to(target, SLIDE);
        if crowded {
            self.arrange();
        }
    }

    pub(in crate::notch) fn crowded_by_menus(&self) -> bool {
        self.dodge.target > 0.0
    }

    pub(in crate::notch) fn lead(&self, left: f32) -> f32 {
        let slid = self.dodge.value;
        if slid <= f32::EPSILON {
            return left;
        }
        let wing = ((self.morph.current().w - self.notch.x) * 0.5).max(1.0);
        let share = (slid / wing).clamp(0.0, 1.0);
        let home = left - slid;
        let beside_camera = CANVAS.x * 0.5 + self.notch.x * 0.5 + CAMERA_CLEARANCE;
        home + (beside_camera - home) * share
    }

    pub(in crate::notch) fn menus_reach(&self, left: f32) -> bool {
        let resting = matches!(self.mode, Mode::Idle | Mode::Activity | Mode::Peek);
        let allowed =
            self.style == Style::Notch && self.preview.is_none() && crate::config::current().notch.dodge_menus;
        self.menu_edge.filter(|_| resting && allowed).is_some_and(|edge| edge + MENU_GAP > left)
    }

    fn dodge_target(&self) -> f32 {
        let g = self.target_geometry();
        let wing = ((g.w - self.notch.x) * 0.5).max(0.0);
        let left = CANVAS.x * 0.5 - g.w * 0.5 - g.ear + self.target_shift();
        if wing > 0.0 && self.menus_reach(left) { wing } else { 0.0 }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use egui::vec2;

    use super::*;
    use crate::notch::live::tests::recording;
    use crate::notch::tests::fixtures::settle;

    fn macbook() -> Notch {
        let mut n = Notch::with_preview(Style::Notch, Some(vec2(185.0, 32.0)), None);
        n.set_activities(vec![recording(notchbase_sdk::now_ms())]);
        n.tick(Instant::now(), None);
        settle(&mut n);
        n
    }

    fn left_wing(n: &Notch) -> f32 {
        CANVAS.x * 0.5 - n.notch.x * 0.5 - n.silhouette().rect().left()
    }

    #[test]
    fn short_menus_leave_it() {
        let mut n = macbook();
        let wing = left_wing(&n);
        assert!(wing > 0.0);
        n.set_menu_edge(Some(CANVAS.x * 0.5 - n.notch.x * 0.5 - wing - 80.0));
        n.step(1.0 / 60.0);
        settle(&mut n);
        assert!((left_wing(&n) - wing).abs() < 0.01);
    }

    #[test]
    fn reaching_menus_slide_it() {
        let mut n = macbook();
        let wing = left_wing(&n);
        n.set_menu_edge(Some(CANVAS.x * 0.5 - n.notch.x * 0.5 - wing * 0.5));
        n.step(1.0 / 60.0);
        settle(&mut n);
        assert!(left_wing(&n).abs() < 0.5, "the left side tucks into the camera housing");
        let leading = n.lead(n.silhouette().rect().left() + 14.0);
        assert!(leading >= CANVAS.x * 0.5 + n.notch.x * 0.5, "what sat on the left now sits right of the camera");
        n.set_menu_edge(None);
        n.step(1.0 / 60.0);
        settle(&mut n);
        assert!((left_wing(&n) - wing).abs() < 0.5, "it flows back once the menus are short again");
    }

    #[test]
    fn panel_and_islands_never_slide() {
        let mut n = macbook();
        n.set_menu_edge(Some(CANVAS.x * 0.5));
        n.set_pinned(true);
        n.step(1.0 / 60.0);
        assert_eq!(n.dodge.target, 0.0);
        let mut island = crate::notch::tests::fixtures::island();
        island.set_activities(vec![recording(1)]);
        island.set_menu_edge(Some(CANVAS.x * 0.5));
        island.step(1.0 / 60.0);
        assert_eq!(island.dodge.target, 0.0);
    }

    #[test]
    fn crowded_keeps_ears_right() {
        let mut n = macbook();
        let other = match recording(2) {
            crate::bridge::Activity::Tool(_, node) => crate::bridge::Activity::Tool(crate::bridge::ToolSlot(4), node),
            other => other,
        };
        n.set_activities(vec![recording(1), crate::notch::live::tests::claude(), other]);
        assert_eq!(n.sides.len(), 2);
        n.set_menu_edge(Some(CANVAS.x * 0.5));
        n.step(1.0 / 60.0);
        assert!(n.sides.iter().filter(|side| side.split.target > 0.0).count() <= 1);
    }
}
