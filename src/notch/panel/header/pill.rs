//! The glass pill that marks the chosen tab. It stretches as it moves and lifts while it is held.

use egui::{Color32, Rect, pos2, vec2};

use crate::ui::glass::{CLEAR, Glass};
use crate::ui::spring::{POINT_PRECISION, Spring, SpringConfig, UNIT_PRECISION, lerp};

const LEAD: SpringConfig = SpringConfig::new(0.24, 0.74);
const TRAIL: SpringConfig = SpringConfig::new(0.46, 0.8);
const LIFT_IN: SpringConfig = SpringConfig::new(0.2, 0.62);
const LIFT_OUT: SpringConfig = SpringConfig::new(0.38, 0.55);
const MOVED: f32 = 0.5;
const THINNEST: f32 = 0.74;
const PLUMPEST: f32 = 1.12;
const LIFT_GROW: f32 = 0.16;
const LIFT_WIDEN: f32 = 4.0;

const RESTING: Glass =
    Glass { tint: Color32::from_rgb(24, 24, 27), sheen: 0.06, specular: 0.7, edge: 0.9, clear: CLEAR };
const LIFTED: Glass =
    Glass { tint: Color32::from_rgb(36, 36, 40), sheen: 0.1, specular: 0.95, edge: 0.95, clear: CLEAR };

pub(in crate::notch) struct LiquidPill {
    left: Spring,
    right: Spring,
    lift: Spring,
}

impl Default for LiquidPill {
    fn default() -> Self {
        let edge = || Spring::new(0.0, LEAD, POINT_PRECISION);
        Self { left: edge(), right: edge(), lift: Spring::new(0.0, LIFT_IN, UNIT_PRECISION) }
    }
}

impl LiquidPill {
    pub(super) fn seat(&mut self, left: f32, right: f32) {
        self.left.snap_to(left);
        self.right.snap_to(right);
    }

    pub(super) fn follow(&mut self, left: f32, right: f32) {
        let moved = (left - self.left.target).abs() > MOVED || (right - self.right.target).abs() > MOVED;
        if !moved {
            return;
        }
        let rightward = left + right > self.left.value + self.right.value;
        let (lead, trail) =
            if rightward { (&mut self.right, &mut self.left) } else { (&mut self.left, &mut self.right) };
        let (lead_to, trail_to) = if rightward { (right, left) } else { (left, right) };
        lead.animate_to(lead_to, LEAD);
        trail.animate_to(trail_to, TRAIL);
    }

    pub(super) fn hold(&mut self, held: bool) {
        let target = if held { 1.0 } else { 0.0 };
        if self.lift.target != target {
            self.lift.animate_to(target, if held { LIFT_IN } else { LIFT_OUT });
        }
    }

    pub(super) fn lift(&self) -> f32 {
        self.lift.value.max(0.0)
    }

    pub(super) fn rect(&self, center_y: f32, height: f32) -> Rect {
        let (left, right) = (self.left.value.min(self.right.value), self.left.value.max(self.right.value));
        let width = (right - left).max(height);
        let rest = (self.right.target - self.left.target).max(height);
        let plump = (rest / width).sqrt().clamp(THINNEST, PLUMPEST);
        let lift = self.lift();
        let h = height * plump * (1.0 + LIFT_GROW * lift);
        let center = pos2((left + right) * 0.5, center_y);
        Rect::from_center_size(center, vec2(width + 2.0 * LIFT_WIDEN * lift, h))
    }

    pub(super) fn glass(&self) -> Glass {
        Glass::lerp(RESTING, LIFTED, self.lift().min(1.0))
    }

    pub(in crate::notch) fn springs(&self) -> [&Spring; 3] {
        [&self.left, &self.right, &self.lift]
    }

    pub(in crate::notch) fn springs_mut(&mut self) -> [&mut Spring; 3] {
        [&mut self.left, &mut self.right, &mut self.lift]
    }
}

pub(super) fn coverage(pill: Rect, slot: Rect) -> f32 {
    let overlap = (pill.right().min(slot.right()) - pill.left().max(slot.left())).max(0.0);
    (overlap / slot.width().min(pill.width()).max(1.0)).clamp(0.0, 1.0)
}

pub(super) fn magnify(lift: f32, covered: f32) -> f32 {
    lerp(1.0, 1.18, lift.min(1.0) * covered)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(pill: &mut LiquidPill, seconds: f32) {
        for _ in 0..(seconds * 120.0) as usize {
            for spring in pill.springs_mut() {
                spring.step(1.0 / 120.0);
            }
        }
    }

    #[test]
    fn leading_edge_stretches_pill() {
        let mut pill = LiquidPill::default();
        pill.seat(0.0, 40.0);
        pill.follow(100.0, 140.0);
        run(&mut pill, 0.08);
        let moving = pill.rect(0.0, 30.0);
        assert!(moving.width() > 40.0, "stretched while travelling");
        assert!(moving.height() < 30.0, "and thinner, like a drop");
        run(&mut pill, 2.0);
        let settled = pill.rect(0.0, 30.0);
        assert!((settled.left() - 100.0).abs() < 0.2 && (settled.right() - 140.0).abs() < 0.2);
        assert!((settled.height() - 30.0).abs() < 0.2);
    }

    #[test]
    fn holding_lifts_the_pill() {
        let mut pill = LiquidPill::default();
        pill.seat(0.0, 40.0);
        pill.hold(true);
        run(&mut pill, 1.0);
        let lifted = pill.rect(0.0, 30.0);
        assert!(lifted.height() > 30.0 && lifted.width() > 40.0);
        pill.hold(false);
        run(&mut pill, 2.0);
        assert!(pill.lift() < 0.01);
    }

    #[test]
    fn coverage_of_a_slot() {
        let slot = Rect::from_min_size(pos2(0.0, 0.0), vec2(30.0, 30.0));
        assert_eq!(coverage(slot, slot), 1.0);
        assert_eq!(coverage(slot.translate(vec2(60.0, 0.0)), slot), 0.0);
        assert!((coverage(slot.translate(vec2(15.0, 0.0)), slot) - 0.5).abs() < 1e-4);
    }
}
