//! Frame timing for the capture overlays.

use std::time::Instant;

pub(in crate::capture) fn elapsed(since: Instant, now: Instant) -> f32 {
    now.saturating_duration_since(since).as_secs_f32()
}

#[derive(Default)]
pub(in crate::capture) struct FrameClock {
    last: Option<Instant>,
}

impl FrameClock {
    pub(in crate::capture) fn restart(&mut self) {
        self.last = None;
    }

    pub(in crate::capture) fn tick(&mut self, now: Instant) -> f32 {
        let dt = frame_dt(self.last, now);
        self.last = Some(now);
        dt
    }
}

/// Time since the last frame, capped at 1/20 s. After a pause over 0.1 s the next step is one 60 Hz frame,
/// so nothing jumps when an idle overlay starts moving again.
fn frame_dt(last: Option<Instant>, now: Instant) -> f32 {
    match last {
        Some(last) => {
            let since = now.saturating_duration_since(last).as_secs_f32();
            if since > 0.1 { 1.0 / 60.0 } else { since.min(1.0 / 20.0) }
        }
        None => 1.0 / 60.0,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn frame_dt_clamps_and_restarts() {
        let now = Instant::now();
        assert_eq!(frame_dt(None, now), 1.0 / 60.0);
        assert_eq!(frame_dt(Some(now - Duration::from_millis(80)), now), 1.0 / 20.0);
        assert_eq!(frame_dt(Some(now - Duration::from_secs(2)), now), 1.0 / 60.0);
    }

    #[test]
    fn clock_measures_from_last_tick() {
        let start = Instant::now();
        let mut clock = FrameClock::default();
        assert_eq!(clock.tick(start), 1.0 / 60.0);
        let dt = clock.tick(start + Duration::from_millis(20));
        assert!((dt - 0.02).abs() < 1e-6);
        clock.restart();
        assert_eq!(clock.tick(start + Duration::from_millis(25)), 1.0 / 60.0);
    }
}
