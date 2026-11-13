//! The notch's state as tools see it, rounded so that small moves don't flood them.

use notchbase_sdk::protocol::{NotchPhase, NotchState, NotchStyle};

use crate::platform::Style;

use super::{Mode, Notch};

const FRACTION_STEPS: f32 = 100.0;
const POINT_STEPS: f32 = 2.0;

impl Notch {
    pub fn state(&self) -> NotchState {
        let geometry = self.morph.current();
        let shape = self.silhouette();
        let island = self.style == Style::Island;
        NotchState {
            phase: phase(self.mode),
            style: if island { NotchStyle::Island } else { NotchStyle::Notch },
            expansion: fraction(self.openness(geometry)),
            hug: fraction(self.hug.value),
            float: if island { fraction(self.attract.value) } else { 0.0 },
            width: points(shape.w),
            height: points(shape.h),
            notch: (!island).then_some([self.notch.x, self.notch.y]),
            held: self.pinned,
            settled: !self.is_animating(),
            music: self.shows_music(),
            ..NotchState::default()
        }
    }
}

fn phase(mode: Mode) -> NotchPhase {
    match mode {
        Mode::Idle => NotchPhase::Resting,
        Mode::Peek => NotchPhase::Peeking,
        Mode::Expanded => NotchPhase::Open,
        Mode::Activity => NotchPhase::Notice,
        Mode::Drop => NotchPhase::Dropping,
        Mode::Ask => NotchPhase::Asking,
    }
}

fn fraction(value: f32) -> f32 {
    (value.clamp(0.0, 1.0) * FRACTION_STEPS).round() / FRACTION_STEPS
}

fn points(value: f32) -> f32 {
    (value.max(0.0) * POINT_STEPS).round() / POINT_STEPS
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::notch::tests::fixtures::{AWAY, island, notch, settle};

    #[test]
    fn resting_island_is_settled() {
        let mut n = island();
        n.tick(Instant::now(), Some(AWAY));
        settle(&mut n);
        let state = n.state();
        assert_eq!((state.phase, state.style), (NotchPhase::Resting, NotchStyle::Island));
        assert_eq!((state.expansion, state.hug, state.float), (0.0, 0.0, 0.0));
        assert!(state.settled && state.resting() && !state.held && state.notch.is_none());
    }

    #[test]
    fn open_notch_reports_full() {
        let mut n = notch();
        n.set_pinned(true);
        settle(&mut n);
        let state = n.state();
        assert_eq!(state.phase, NotchPhase::Open);
        assert!(state.fully_open() && state.held && state.notch.is_some());
        assert!(state.width > state.notch.unwrap()[0]);
    }

    #[test]
    fn taller_page_stays_fully_open() {
        let mut n = notch();
        n.set_pinned(true);
        settle(&mut n);
        n.pager.remeasure(crate::notch::panel::pager::Heights::from([(crate::bridge::Page::Settings, 400.0)]));
        n.switch_to(crate::bridge::Page::Settings);
        n.step(1.0 / 60.0);
        assert!(n.open_progress(n.morph.current()) < 0.5, "the shape has only begun to grow");
        assert_eq!(n.state().expansion, 1.0, "but the panel is as open as ever, shadow and all");
    }

    #[test]
    fn halfway_values_are_rounded() {
        assert_eq!(fraction(0.123_456), 0.12);
        assert_eq!(fraction(1.3), 1.0);
        assert_eq!(points(180.26), 180.5);
    }
}
