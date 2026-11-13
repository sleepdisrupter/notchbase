//! Dragging an icon to a new place in the row.

use std::collections::HashMap;

use egui::{Pos2, Vec2, pos2};

use crate::bridge::ToolSlot;
use crate::platform::{Feel, haptic};
use crate::ui::motion;
use crate::ui::spring::{POINT_PRECISION, Spring, UNIT_PRECISION, lerp};

const LIFTED: f32 = 1.12;

struct Held {
    slot: ToolSlot,
    grip: Vec2,
    at: Pos2,
}

pub(super) struct Reorder {
    held: Option<Held>,
    order: Option<Vec<ToolSlot>>,
    places: HashMap<ToolSlot, [Spring; 2]>,
    raised: Option<ToolSlot>,
    lift: Spring,
}

impl Default for Reorder {
    fn default() -> Self {
        Self {
            held: None,
            order: None,
            places: HashMap::new(),
            raised: None,
            lift: Spring::new(0.0, motion::HOVER, UNIT_PRECISION),
        }
    }
}

impl Reorder {
    pub(super) fn arrange(&mut self, listed: &[ToolSlot]) -> Vec<ToolSlot> {
        self.places.retain(|slot, _| listed.contains(slot));
        let same = |order: &[ToolSlot]| order.len() == listed.len() && order.iter().all(|slot| listed.contains(slot));
        match &self.order {
            Some(order) if !same(order) => self.forget(),
            Some(order) if self.held.is_none() && order == listed => self.order = None,
            _ => {}
        }
        self.order.clone().unwrap_or_else(|| listed.to_vec())
    }

    pub(super) fn grab(&mut self, slot: ToolSlot, cell: Pos2, pointer: Pos2, listed: &[ToolSlot]) {
        self.held = Some(Held { slot, grip: cell - pointer, at: cell });
        self.order = Some(listed.to_vec());
        self.raised = Some(slot);
        self.lift.animate_to(1.0, motion::HOVER);
        haptic(Feel::Tick);
    }

    pub(super) fn drag(&mut self, pointer: Pos2, over: usize) {
        let (Some(held), Some(order)) = (&mut self.held, &mut self.order) else { return };
        held.at = pointer + held.grip;
        let Some(from) = order.iter().position(|slot| *slot == held.slot) else { return };
        let to = over.min(order.len() - 1);
        if from != to {
            let slot = order.remove(from);
            order.insert(to, slot);
            haptic(Feel::Tick);
        }
    }

    pub(super) fn drop(&mut self, listed: &[ToolSlot]) -> Option<(ToolSlot, Option<ToolSlot>)> {
        let held = self.held.take()?;
        self.lift.animate_to(0.0, motion::SMOOTH);
        let order = self.order.as_ref().filter(|order| order.as_slice() != listed);
        let Some(order) = order else {
            self.order = None;
            return None;
        };
        haptic(Feel::Snap);
        let after = order.iter().skip_while(|slot| **slot != held.slot).nth(1).copied();
        Some((held.slot, after))
    }

    pub(super) fn place(&mut self, slot: ToolSlot, home: Pos2) -> Pos2 {
        let held = self.held.as_ref().filter(|held| held.slot == slot).map(|held| held.at);
        let place = self.places.entry(slot).or_insert_with(|| [home.x, home.y].map(point));
        match held {
            Some(at) => {
                place[0].snap_to(at.x);
                place[1].snap_to(at.y);
            }
            None => {
                place[0].animate_to(home.x, motion::SMOOTH);
                place[1].animate_to(home.y, motion::SMOOTH);
            }
        }
        pos2(place[0].value, place[1].value)
    }

    pub(super) fn holding(&self) -> Option<ToolSlot> {
        self.held.as_ref().map(|held| held.slot)
    }

    pub(super) fn raised(&self) -> Option<ToolSlot> {
        self.raised.filter(|_| self.held.is_some() || !self.settled())
    }

    pub(super) fn scale(&self, slot: ToolSlot, pressed: f32) -> f32 {
        if self.raised == Some(slot) { lerp(pressed, LIFTED, self.lift.value.clamp(0.0, 1.0)) } else { pressed }
    }

    pub(super) fn forget(&mut self) {
        self.held = None;
        self.order = None;
        self.raised = None;
        self.lift.snap_to(0.0);
    }

    pub(super) fn rest(&mut self) {
        self.forget();
        self.places.clear();
    }

    pub(super) fn step(&mut self, dt: f32) {
        self.springs_mut().for_each(|spring| spring.step(dt));
    }

    pub(super) fn snap(&mut self) {
        self.springs_mut().for_each(|spring| spring.snap_to(spring.target));
    }

    pub(super) fn is_animating(&self) -> bool {
        self.held.is_some() || !self.settled()
    }

    fn settled(&self) -> bool {
        self.lift.is_settled() && self.places.values().flatten().all(Spring::is_settled)
    }

    fn springs_mut(&mut self) -> impl Iterator<Item = &mut Spring> {
        self.places.values_mut().flatten().chain([&mut self.lift])
    }
}

fn point(value: f32) -> Spring {
    Spring::new(value, motion::SMOOTH, POINT_PRECISION)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LISTED: [ToolSlot; 4] = [ToolSlot(1), ToolSlot(2), ToolSlot(3), ToolSlot(4)];
    const FRAME: f32 = 1.0 / 60.0;

    fn slots<const N: usize>(slots: [u16; N]) -> Vec<ToolSlot> {
        slots.map(ToolSlot).to_vec()
    }

    fn holding(slot: u16) -> Reorder {
        let mut reorder = Reorder::default();
        reorder.grab(ToolSlot(slot), pos2(100.0, 50.0), pos2(120.0, 70.0), &LISTED);
        reorder
    }

    #[test]
    fn order_stands_until_pickup() {
        let mut reorder = Reorder::default();
        assert_eq!(reorder.arrange(&LISTED), LISTED);
        assert_eq!((reorder.holding(), reorder.raised()), (None, None));
        assert!(!reorder.is_animating());
    }

    #[test]
    fn dragging_makes_room() {
        let mut reorder = holding(1);
        reorder.drag(pos2(300.0, 70.0), 2);
        assert_eq!(reorder.arrange(&LISTED), slots([2, 3, 1, 4]));
        reorder.drag(pos2(900.0, 70.0), 99);
        assert_eq!(reorder.arrange(&LISTED), slots([2, 3, 4, 1]), "past the end is the last slot");
        reorder.drag(pos2(0.0, 70.0), 0);
        assert_eq!(reorder.arrange(&LISTED), LISTED);
    }

    #[test]
    fn held_icon_keeps_grip() {
        let mut reorder = holding(2);
        reorder.drag(pos2(220.0, 90.0), 1);
        assert_eq!(reorder.place(ToolSlot(2), pos2(100.0, 50.0)), pos2(200.0, 70.0));
        assert_eq!(reorder.scale(ToolSlot(3), 0.96), 0.96);
        reorder.lift.snap_to(1.0);
        assert_eq!(reorder.scale(ToolSlot(2), 0.96), LIFTED);
    }

    #[test]
    fn drop_names_next_tool() {
        let mut reorder = holding(4);
        reorder.drag(pos2(10.0, 10.0), 1);
        assert_eq!(reorder.drop(&LISTED), Some((ToolSlot(4), Some(ToolSlot(2)))));
        assert_eq!(reorder.arrange(&LISTED), slots([1, 4, 2, 3]), "it holds the new order until the tools follow");
        assert_eq!(reorder.arrange(&slots([1, 4, 2, 3])), slots([1, 4, 2, 3]));
        assert!(reorder.order.is_none());
        let mut last = holding(1);
        last.drag(pos2(900.0, 10.0), 3);
        assert_eq!(last.drop(&LISTED), Some((ToolSlot(1), None)));
    }

    #[test]
    fn drop_in_place_changes_nothing() {
        let mut reorder = holding(3);
        reorder.drag(pos2(10.0, 10.0), 0);
        reorder.drag(pos2(250.0, 10.0), 2);
        assert_eq!(reorder.drop(&LISTED), None);
        assert_eq!(reorder.arrange(&LISTED), LISTED);
        assert_eq!(reorder.drop(&LISTED), None, "nothing is held any more");
    }

    #[test]
    fn icons_glide_to_new_slots() {
        let mut reorder = holding(1);
        let (home, next) = (pos2(100.0, 50.0), pos2(185.0, 50.0));
        assert_eq!(reorder.place(ToolSlot(2), next), next, "a slot seen for the first time starts in place");
        let moved = reorder.place(ToolSlot(2), home);
        assert_eq!(moved, next, "then it starts from where it was");
        reorder.step(FRAME);
        let gliding = reorder.place(ToolSlot(2), home).x;
        assert!(gliding < next.x && gliding > home.x, "{gliding}");
        reorder.drag(pos2(320.0, 70.0), 1);
        reorder.place(ToolSlot(1), next);
        reorder.drop(&LISTED);
        assert_eq!(reorder.raised(), Some(ToolSlot(1)), "it stays on top while it settles");
        for _ in 0..240 {
            reorder.place(ToolSlot(1), next);
            reorder.place(ToolSlot(2), home);
            reorder.step(FRAME);
        }
        assert_eq!(reorder.place(ToolSlot(1), next), next);
        assert_eq!(reorder.raised(), None);
        assert!(!reorder.is_animating());
    }

    #[test]
    fn tool_change_ends_drag() {
        let mut reorder = holding(2);
        reorder.drag(pos2(10.0, 10.0), 0);
        assert_eq!(reorder.arrange(&slots([1, 2, 3])), slots([1, 2, 3]));
        assert_eq!(reorder.holding(), None);
        assert_eq!(reorder.drop(&slots([1, 2, 3])), None);
    }
}
