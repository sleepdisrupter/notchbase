//! The launch motion: an opened app's icon swells, then rises toward the notch and fades.

use std::time::Instant;

use egui::Pos2;

use crate::ui::spring::smoothstep;

pub(super) const LASTS: f32 = 0.7;
pub(super) const PIN_SWELLS_TO: f32 = 1.15;
const FLIGHT: f32 = 0.27;
const CLEARS_IN: f32 = 0.14;
const SWELLS_BY: f32 = 0.34;
const SWELLS_FOR: f32 = 0.4;
const LEAVES_AT: f32 = 0.2;
const ARRIVES_AT: f32 = 0.5;
const FADES_FROM: f32 = 0.84;
pub(super) const HALO: [(f32, f32); 3] = [(0.72, 0.075), (0.92, 0.045), (1.16, 0.022)];

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Launch {
    pub(super) key: u64,
    pub(super) home: Option<Pos2>,
    pub(super) grown: f32,
    since: f32,
    at: Instant,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Pose {
    pub(super) at: Pos2,
    pub(super) scale: f32,
    pub(super) alpha: f32,
    pub(super) glow: f32,
}

impl Launch {
    pub(super) fn of(key: u64, now: Instant) -> Self {
        Self { key, home: None, grown: 1.0, since: 0.0, at: now }
    }

    pub(super) fn from(key: u64, (home, grown): (Pos2, f32), now: Instant) -> Self {
        Self { home: Some(home), grown, ..Self::of(key, now) }
    }

    pub(super) fn step(&mut self, dt: f32) {
        self.since += dt;
    }

    pub(super) fn over(&self) -> bool {
        self.since >= LASTS || self.at.elapsed().as_secs_f32() >= LASTS
    }

    pub(super) fn left_of_the_rest(&self) -> f32 {
        1.0 - smoothstep(0.0, CLEARS_IN, self.since)
    }

    pub(super) fn pose(&self, home: Pos2, goal: Pos2) -> Pose {
        let flown = (self.since / FLIGHT).clamp(0.0, 1.0);
        let swollen = 1.0 - (1.0 - (flown / SWELLS_FOR).min(1.0)).powi(3);
        let risen = smoothstep(LEAVES_AT, 1.0, flown);
        Pose {
            at: home + (goal - home) * risen,
            scale: 1.0 + SWELLS_BY * swollen - (SWELLS_BY + 1.0 - ARRIVES_AT) * risen,
            alpha: 1.0 - smoothstep(FADES_FROM, 1.0, flown),
            glow: swollen * (1.0 - risen),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use egui::pos2;

    use super::*;

    fn after(seconds: f32) -> Launch {
        let mut launch = Launch::of(7, Instant::now());
        launch.step(seconds);
        launch
    }

    #[test]
    fn icon_swells_then_rises_away() {
        let (home, goal) = (pos2(30.0, 200.0), pos2(170.0, 0.0));
        let at = |seconds: f32| after(seconds).pose(home, goal);
        assert_eq!(at(0.0), Pose { at: home, scale: 1.0, alpha: 1.0, glow: 0.0 });
        let lifted = at(FLIGHT * LEAVES_AT);
        assert!(lifted.at == home && lifted.scale > 1.2 && lifted.glow > 0.8, "it grows before it moves: {lifted:?}");
        let halfway = at(FLIGHT * 0.6);
        assert!(halfway.at.y < home.y && halfway.at.y > goal.y && halfway.alpha == 1.0);
        assert!(halfway.scale < lifted.scale);
        let arrived = at(FLIGHT);
        assert_eq!((arrived.at, arrived.alpha, arrived.glow), (goal, 0.0, 0.0));
        assert!((arrived.scale - ARRIVES_AT).abs() < 1e-4, "small enough for the notch to take in");
        assert_eq!(at(FLIGHT * 3.0), arrived, "and it stays there");
        let scales: Vec<f32> = (0..=27).map(|step| at(step as f32 * 0.01).scale).collect();
        assert!(scales.iter().all(|scale| (ARRIVES_AT - 1e-4..=1.0 + SWELLS_BY).contains(scale)));
    }

    #[test]
    fn launch_ends_by_frames_or_clock() {
        assert_eq!(after(0.0).left_of_the_rest(), 1.0);
        assert!(after(CLEARS_IN * 0.5).left_of_the_rest() < 0.6);
        assert_eq!(after(CLEARS_IN).left_of_the_rest(), 0.0);
        assert!(!after(FLIGHT).over() && after(LASTS).over());
        let long_ago = Launch { at: Instant::now() - Duration::from_secs(2), ..after(0.0) };
        assert!(long_ago.over(), "a page that stopped painting does not keep it going");
    }
}
