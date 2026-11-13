//! Turns scrolling into page turns: one per trackpad swipe however far it glides, and one per wheel notch.

use egui::{Id, Vec2, vec2};

const REACH: f32 = 1.2;
const QUIET: f64 = 0.28;
const BETWEEN_NOTCHES: f64 = 0.15;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Gesture {
    travel: Vec2,
    last: f64,
    touch: u64,
    spent: bool,
}

pub fn notch(ui: &egui::Ui, id: Id, notches: f32) -> i32 {
    if notches == 0.0 {
        return 0;
    }
    let now = ui.input(|input| input.time);
    let id = id.with("notch");
    let last = ui.ctx().data(|data| data.get_temp::<f64>(id));
    if last.is_some_and(|last| now - last < BETWEEN_NOTCHES) {
        return 0;
    }
    ui.ctx().data_mut(|data| data.insert_temp(id, now));
    -(notches.signum() as i32)
}

pub fn once(ui: &egui::Ui, id: Id, glide: f32) -> i32 {
    either(ui, id, vec2(glide, 0.0))
}

pub fn either(ui: &egui::Ui, id: Id, glide: Vec2) -> i32 {
    let now = ui.input(|input| input.time);
    let id = id.with("swipe");
    let gesture = ui.ctx().data_mut(|data| data.get_temp::<Gesture>(id)).unwrap_or_default();
    let (step, next) = advance(gesture, glide, now, crate::ui::scroll::touch(ui));
    ui.ctx().data_mut(|data| data.insert_temp(id, next));
    step
}

fn advance(mut gesture: Gesture, glide: Vec2, now: f64, touch: u64) -> (i32, Gesture) {
    // A pause or a new touch on the trackpad starts a fresh swipe.
    if now - gesture.last > QUIET || touch != gesture.touch {
        gesture = Gesture { touch, ..Gesture::default() };
    }
    gesture.last = now;
    if gesture.spent {
        return (0, gesture);
    }
    gesture.travel += glide;
    let travel = if gesture.travel.x.abs() >= gesture.travel.y.abs() { gesture.travel.x } else { gesture.travel.y };
    if travel.abs() < REACH {
        return (0, gesture);
    }
    gesture.travel = Vec2::ZERO;
    gesture.spent = true;
    (-(travel.signum() as i32), gesture)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn swipe(deltas: &[(f32, f64)]) -> Vec<i32> {
        touches(&deltas.iter().map(|&(glide, at)| (vec2(glide, 0.0), at, 0)).collect::<Vec<_>>())
    }

    fn touches(deltas: &[(Vec2, f64, u64)]) -> Vec<i32> {
        let mut gesture = Gesture::default();
        deltas
            .iter()
            .map(|(glide, at, touch)| {
                let (step, next) = advance(gesture, *glide, *at, *touch);
                gesture = next;
                step
            })
            .collect()
    }

    #[test]
    fn one_swipe_turns_one_page() {
        let steps = swipe(&[(-0.5, 1.0), (-0.5, 1.02), (-0.5, 1.04), (-0.5, 1.06), (-0.3, 1.2), (-0.1, 1.4)]);
        assert_eq!(steps, [0, 0, 1, 0, 0, 0]);
    }

    #[test]
    fn pause_starts_a_new_swipe() {
        let steps = swipe(&[(2.0, 1.0), (2.0, 1.1), (2.0, 1.5)]);
        assert_eq!(steps, [-1, 0, -1]);
    }

    #[test]
    fn small_nudge_does_nothing() {
        assert_eq!(swipe(&[(0.4, 1.0), (0.3, 1.05)]), [0, 0]);
    }

    #[test]
    fn new_touch_turns_again() {
        let across = |x: f32| vec2(x, 0.0);
        let steps = touches(&[
            (across(-2.0), 1.0, 1),
            (across(-0.4), 1.1, 1),
            (across(-0.2), 1.2, 1),
            (across(-2.0), 1.3, 2),
            (across(-0.4), 1.4, 2),
        ]);
        assert_eq!(steps, [1, 0, 0, 1, 0], "the coasting is spent, the second touch is not");
        let leftover = touches(&[(across(-0.8), 1.0, 1), (across(0.8), 1.1, 2)]);
        assert_eq!(leftover, [0, 0], "a new touch starts from nothing");
    }

    #[test]
    fn longer_axis_decides() {
        let steps = touches(&[(vec2(0.2, -0.7), 1.0, 0), (vec2(-0.3, -0.7), 1.05, 0)]);
        assert_eq!(steps, [0, 1], "a wobble sideways does not cancel a swipe up");
    }
}
