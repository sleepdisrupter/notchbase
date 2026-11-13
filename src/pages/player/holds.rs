//! A seek or a play/pause shows at once and holds until the player catches up.

use std::time::{Duration, Instant};

const HOLD_FOR: Duration = Duration::from_millis(2500);
const CONFIRMED: f64 = 1.5;

#[derive(Default)]
pub(super) struct Holds {
    position: Option<(f64, Instant)>,
    playing: Option<(bool, Instant)>,
}

impl Holds {
    pub(super) fn is_holding(&self) -> bool {
        self.position.is_some() || self.playing.is_some()
    }

    pub(super) fn hold_position(&mut self, seconds: f64, now: Instant) {
        self.position = Some((seconds, now));
    }

    pub(super) fn hold_playing(&mut self, playing: bool, now: Instant) {
        self.playing = Some((playing, now));
    }

    pub(super) fn position(&mut self, actual: f64, playing: bool, now: Instant) -> f64 {
        let Some((held, since)) = self.position else { return actual };
        let elapsed = now.saturating_duration_since(since);
        let expected = held + if playing { elapsed.as_secs_f64() } else { 0.0 };
        if elapsed >= HOLD_FOR || (actual - expected).abs() < CONFIRMED {
            self.position = None;
            return actual;
        }
        expected
    }

    pub(super) fn playing(&mut self, actual: bool, now: Instant) -> bool {
        let Some((held, since)) = self.playing else { return actual };
        if actual == held || now.saturating_duration_since(since) >= HOLD_FOR {
            self.playing = None;
            return actual;
        }
        held
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seek_shows_at_once() {
        let t0 = Instant::now();
        let mut holds = Holds::default();
        holds.hold_position(90.0, t0);
        assert_eq!(holds.position(10.0, false, t0 + Duration::from_millis(300)), 90.0);
        let later = t0 + Duration::from_secs(1);
        assert_eq!(holds.position(10.0, true, later), 91.0);
        assert_eq!(holds.position(91.2, true, later), 91.2);
        assert!(!holds.is_holding());
        holds.hold_position(30.0, t0);
        assert_eq!(holds.position(5.0, false, t0 + HOLD_FOR), 5.0, "gives up eventually");
    }

    #[test]
    fn toggle_shows_at_once() {
        let t0 = Instant::now();
        let mut holds = Holds::default();
        holds.hold_playing(false, t0);
        assert!(!holds.playing(true, t0));
        assert!(!holds.playing(false, t0));
        assert!(holds.playing(true, t0), "confirmed, so the player wins again");
    }
}
