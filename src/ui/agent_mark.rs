//! The Claude or Codex mark beside a running agent. It turns slowly while thinking, quickly while doing something,
//! and stands still in yellow while it waits for you.

use std::f32::consts::TAU;

use egui::{Color32, Pos2};

use crate::assistant::Provider;
use crate::bridge::Mood;

use super::brand;
use super::canvas::Canvas;
use super::color;
use super::spring::{Spring, SpringConfig, UNIT_PRECISION};

pub const WAITING: Color32 = color::YELLOW;
const CODEX: Color32 = Color32::WHITE;
const THINKING_SPIN: f32 = 0.55;
const DOING_SPIN: f32 = 2.4;
const DOING_PULSE: f32 = 0.08;
const DOING_BREATH: f32 = 6.0;
const SECOND_MS: u64 = 1000;
const EASE: SpringConfig = SpringConfig::new(0.6, 1.0);

#[derive(Clone, Copy, Debug)]
pub struct Spin {
    turn: f32,
    breath: f32,
    rate: Spring,
    swell: Spring,
    driven: bool,
}

impl Default for Spin {
    fn default() -> Self {
        let eased = || Spring::new(0.0, EASE, UNIT_PRECISION);
        Self { turn: 0.0, breath: 0.0, rate: eased(), swell: eased(), driven: false }
    }
}

impl Spin {
    pub fn follow(&mut self, mood: Mood, dt: f32) {
        let (rate, swell) = match mood {
            Mood::Thinking => (THINKING_SPIN, 0.0),
            Mood::Doing(_) => (DOING_SPIN, 1.0),
            Mood::Waiting => (0.0, 0.0),
        };
        self.rate.animate_to(rate, EASE);
        self.swell.animate_to(swell, EASE);
        // The first mood snaps into place. Only later changes ease.
        if !std::mem::replace(&mut self.driven, true) {
            self.rate.snap_to(rate);
            self.swell.snap_to(swell);
        }
        self.rate.step(dt);
        self.swell.step(dt);
        self.turn = (self.turn + self.rate.value * dt) % TAU;
        self.breath = (self.breath + DOING_BREATH * dt) % TAU;
    }

    pub fn is_settled(&self) -> bool {
        self.rate.is_settled() && self.swell.is_settled()
    }
}

pub fn mark(canvas: &mut Canvas, provider: Provider, mood: Mood, center: Pos2, size: f32, spin: &Spin) {
    let (turn, size) = (spin.turn, size * (1.0 + DOING_PULSE * spin.swell.value * spin.breath.sin()));
    let tint = match (mood, provider) {
        (Mood::Waiting, _) => WAITING,
        (_, Provider::Claude) => color::CLAUDE,
        (_, Provider::Codex) => CODEX,
    };
    match provider {
        Provider::Claude => brand::spark(canvas, center, size, tint, turn),
        Provider::Codex => brand::knot(canvas, center, size, tint, turn),
    }
}

pub fn mood(waiting: bool, doing: Option<&'static str>) -> Mood {
    match (waiting, doing) {
        (true, _) => Mood::Waiting,
        (false, Some(verb)) => Mood::Doing(verb),
        (false, None) => Mood::Thinking,
    }
}

pub fn moves(mood: Mood) -> bool {
    mood != Mood::Waiting
}

pub fn clock(elapsed_ms: u64) -> String {
    let seconds = elapsed_ms / SECOND_MS;
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if hours > 0 { format!("{hours}:{minutes:02}:{seconds:02}") } else { format!("{minutes}:{seconds:02}") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_reads_minutes_then_hours() {
        assert_eq!(clock(42_000), "0:42");
        assert_eq!(clock(192_000), "3:12");
        assert_eq!(clock(3_699_000), "1:01:39");
    }

    #[test]
    fn mood_change_eases_the_spin() {
        let mut spin = Spin::default();
        spin.follow(Mood::Thinking, 1.0 / 60.0);
        assert!((spin.rate.value - THINKING_SPIN).abs() < 1e-6, "a fresh mark spins at its mood's pace at once");
        for _ in 0..600 {
            spin.follow(Mood::Thinking, 1.0 / 60.0);
        }
        for mood in [Mood::Doing("Editing"), Mood::Thinking, Mood::Waiting, Mood::Doing("Reading")] {
            for _ in 0..120 {
                let (turn, swell) = (spin.turn, spin.swell.value);
                spin.follow(mood, 1.0 / 60.0);
                let turned = (spin.turn - turn).rem_euclid(TAU);
                assert!(turned < DOING_SPIN / 60.0 + 1e-3, "{mood:?} turned {turned} in one frame");
                assert!((spin.swell.value - swell).abs() < 0.08, "{mood:?} swelled in one frame");
            }
        }
        for _ in 0..120 {
            spin.follow(Mood::Waiting, 1.0 / 60.0);
        }
        assert!(spin.is_settled() && spin.rate.value.abs() < 1e-3, "waiting slows the mark to a stop");
    }

    #[test]
    fn waiting_wins_and_stands_still() {
        assert_eq!(mood(true, Some("Editing")), Mood::Waiting);
        assert_eq!(mood(false, Some("Editing")), Mood::Doing("Editing"));
        assert_eq!(mood(false, None), Mood::Thinking);
        assert!(moves(Mood::Thinking) && moves(Mood::Doing("Running")));
        assert!(!moves(Mood::Waiting));
    }
}
