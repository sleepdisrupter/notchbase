//! Wheel and trackpad input, shared by every control that scrolls.

use egui::{Id, MouseWheelUnit, TouchPhase};

pub const POINTS_PER_NOTCH: f32 = 40.0;
const PAGE_NOTCHES: f32 = 10.0;
const WHOLE: f32 = 0.01;
const CLAIMED: &str = "notchbase.scroll.claimed";
const PAGE_SCROLLS: &str = "notchbase.scroll.page";
const TOUCH: &str = "notchbase.scroll.touch";
const COASTS_WITHIN: f64 = 0.12; // momentum that starts this soon after a lift belongs to the same swipe

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Wheel {
    pub x: f32,
    pub y: f32,
    pub smooth: bool,
}

impl Wheel {
    pub fn down(&self) -> f32 {
        if self.x.abs() > self.y.abs() { 0.0 } else { self.y }
    }

    pub fn across(&self) -> f32 {
        match self.smooth {
            false => self.y + self.x,
            true if self.x.abs() > self.y.abs() => self.x,
            true => 0.0,
        }
    }

    fn add(&mut self, unit: MouseWheelUnit, delta: egui::Vec2) {
        let scale = match unit {
            MouseWheelUnit::Point => 1.0 / POINTS_PER_NOTCH,
            MouseWheelUnit::Line => 1.0,
            MouseWheelUnit::Page => PAGE_NOTCHES,
        };
        let partial = |value: f32| (value - value.round()).abs() > WHOLE;
        self.smooth |= unit == MouseWheelUnit::Point || partial(delta.x) || partial(delta.y);
        self.x += delta.x * scale;
        self.y += delta.y * scale;
    }
}

pub fn read(ui: &egui::Ui) -> Wheel {
    ui.input(|input| {
        input.events.iter().fold(Wheel::default(), |mut wheel, event| {
            if let egui::Event::MouseWheel { unit, delta, .. } = event {
                wheel.add(*unit, *delta);
            }
            wheel
        })
    })
}

pub fn across(ui: &egui::Ui) -> f32 {
    take(ui, read(ui).across())
}

pub fn across_points(ui: &egui::Ui) -> f32 {
    across(ui) * POINTS_PER_NOTCH
}

pub fn down(ui: &egui::Ui) -> f32 {
    take(ui, read(ui).down())
}

pub fn take(ui: &egui::Ui, used: f32) -> f32 {
    if used != 0.0 {
        claim(ui);
    }
    used
}

/// Marks the wheel as used this frame, so the page behind doesn't scroll as well.
pub fn claim(ui: &egui::Ui) {
    let pass = ui.ctx().cumulative_pass_nr();
    ui.ctx().data_mut(|data| data.insert_temp(Id::new(CLAIMED), pass));
}

pub fn claimed(ui: &egui::Ui) -> bool {
    let pass = ui.ctx().cumulative_pass_nr();
    ui.ctx().data(|data| data.get_temp::<u64>(Id::new(CLAIMED))) == Some(pass)
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Fingers {
    #[default]
    Away,
    Down {
        moved: bool,
    },
    Lifted {
        at: f64,
    },
    Coasting,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Touch {
    fingers: Fingers,
    serial: u64,
}

impl Touch {
    fn after(mut self, phase: TouchPhase, now: f64) -> Self {
        self.fingers = match (phase, self.fingers) {
            (TouchPhase::Start, Fingers::Lifted { at }) if now - at < COASTS_WITHIN => Fingers::Coasting,
            (TouchPhase::Start, Fingers::Down { moved: false }) => Fingers::Down { moved: false },
            (TouchPhase::Start, _) => {
                self.serial += 1;
                Fingers::Down { moved: false }
            }
            (TouchPhase::Move, Fingers::Down { .. }) => Fingers::Down { moved: true },
            (TouchPhase::End | TouchPhase::Cancel, Fingers::Down { .. }) => Fingers::Lifted { at: now },
            (TouchPhase::End | TouchPhase::Cancel, Fingers::Coasting) => Fingers::Away,
            (_, fingers) => fingers,
        };
        self
    }
}

/// Follows trackpad touches, so a swipe and its momentum count as one gesture.
pub fn track(ui: &egui::Ui) {
    let id = Id::new(TOUCH);
    let touch = ui.ctx().data(|data| data.get_temp::<Touch>(id)).unwrap_or_default();
    let next = ui.input(|input| {
        input.events.iter().fold(touch, |touch, event| match event {
            egui::Event::MouseWheel { phase, .. } => touch.after(*phase, input.time),
            _ => touch,
        })
    });
    if next != touch {
        ui.ctx().data_mut(|data| data.insert_temp(id, next));
    }
}

pub fn touch(ui: &egui::Ui) -> u64 {
    ui.ctx().data(|data| data.get_temp::<Touch>(Id::new(TOUCH))).map_or(0, |touch| touch.serial)
}

pub fn set_page_scrolls(ui: &egui::Ui, scrolls: bool) {
    ui.ctx().data_mut(|data| data.insert_temp(Id::new(PAGE_SCROLLS), scrolls));
}

pub fn page_scrolls(ui: &egui::Ui) -> bool {
    ui.ctx().data(|data| data.get_temp::<bool>(Id::new(PAGE_SCROLLS))).unwrap_or(false)
}

/// Adds up partial wheel notches and hands out whole ones.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Notches {
    left: f32,
}

impl Notches {
    pub fn whole(&mut self, notches: f32) -> i32 {
        self.left += notches;
        let whole = self.left.trunc();
        self.left -= whole;
        whole as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trackpad_splits_by_direction() {
        let sideways = Wheel { x: 2.0, y: 0.3, smooth: true };
        assert_eq!((sideways.across(), sideways.down()), (2.0, 0.0));
        let upward = Wheel { x: 0.2, y: 1.5, smooth: true };
        assert_eq!((upward.across(), upward.down()), (0.0, 1.5));
    }

    fn wheel(events: &[(MouseWheelUnit, f32, f32)]) -> Wheel {
        events.iter().fold(Wheel::default(), |mut wheel, (unit, x, y)| {
            wheel.add(*unit, egui::vec2(*x, *y));
            wheel
        })
    }

    #[test]
    fn fractional_steps_are_smooth() {
        assert!(!wheel(&[(MouseWheelUnit::Line, 0.0, -1.0), (MouseWheelUnit::Line, 0.0, -2.0)]).smooth);
        assert!(wheel(&[(MouseWheelUnit::Line, 0.0, -0.125)]).smooth, "a touchpad sends fractions of a notch");
        assert!(wheel(&[(MouseWheelUnit::Point, 0.0, 12.0)]).smooth);
        assert_eq!(wheel(&[(MouseWheelUnit::Point, 0.0, 80.0)]).y, 2.0);
    }

    #[test]
    fn coasting_belongs_to_its_touch() {
        use TouchPhase::{Cancel, End, Move, Start};
        let run = |touch: Touch, phases: &[(TouchPhase, f64)]| {
            phases.iter().fold(touch, |touch, (phase, at)| touch.after(*phase, *at))
        };
        let swiped = run(Touch::default(), &[(Start, 1.0), (Start, 1.0), (Move, 1.05), (End, 1.2)]);
        assert_eq!(swiped.serial, 1, "resting the fingers and then moving them is one touch");
        let coasting = run(swiped, &[(Start, 1.21), (Move, 1.3), (Move, 1.9)]);
        assert_eq!((coasting.serial, coasting.fingers), (1, Fingers::Coasting));
        let caught = run(coasting, &[(Cancel, 2.0), (Start, 2.01), (Move, 2.05)]);
        assert_eq!(caught.serial, 2, "touching again while it coasts is a new swipe");
        let later = run(run(Touch::default(), &[(Start, 1.0), (Move, 1.1), (End, 1.2)]), &[(Start, 3.0)]);
        assert_eq!(later.serial, 2, "a swipe that did not coast is still over");
        assert_eq!(run(Touch::default(), &[(Move, 1.0), (Move, 1.1)]).serial, 0, "a wheel has no touches");
    }

    #[test]
    fn wheel_drives_sideways_controls() {
        let notch = Wheel { x: 0.0, y: -1.0, smooth: false };
        assert_eq!((notch.across(), notch.down()), (-1.0, -1.0));
    }

    #[test]
    fn small_steps_add_up() {
        let mut notches = Notches::default();
        let steps: Vec<i32> = [0.3, 0.3, 0.3, 0.3, -0.5, -0.9].into_iter().map(|step| notches.whole(step)).collect();
        assert_eq!(steps, [0, 0, 0, 1, 0, -1]);
    }
}
