//! Frame time for animation: a sixtieth of a second after a rest, otherwise the real gap up to a twentieth.

use std::time::Instant;

const NOMINAL: f32 = 1.0 / 60.0;
const LONGEST: f32 = 1.0 / 20.0;
const IDLE_AFTER: f32 = 0.1;

#[derive(Default)]
pub(super) struct Clock {
    last: Option<Instant>,
}

impl Clock {
    pub(super) fn tick(&mut self, now: Instant) -> f32 {
        let since = self.last.map(|last| now.saturating_duration_since(last).as_secs_f32());
        self.last = Some(now);
        since.map_or(NOMINAL, frame_dt)
    }
}

fn frame_dt(since: f32) -> f32 {
    if since > IDLE_AFTER { NOMINAL } else { since.min(LONGEST) }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn first_and_rested_frames_are_nominal() {
        let start = Instant::now();
        let mut clock = Clock::default();
        assert_eq!(clock.tick(start), NOMINAL);
        assert_eq!(clock.tick(start + Duration::from_secs(3)), NOMINAL);
    }

    #[test]
    fn slow_frames_are_capped() {
        let start = Instant::now();
        let mut clock = Clock::default();
        clock.tick(start);
        assert!((clock.tick(start + Duration::from_millis(8)) - 0.008).abs() < 1e-6);
        assert_eq!(clock.tick(start + Duration::from_millis(88)), LONGEST);
        assert_eq!(clock.tick(start + Duration::from_millis(80)), 0.0, "a clock running backwards moves nothing");
    }
}
