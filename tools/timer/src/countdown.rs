//! The countdown, and the short text it is saved as so a running timer survives a restart.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Countdown {
    Idle,
    Running { start: u64, end: u64 },
    Paused { left: u64, length: u64 },
}

impl Countdown {
    pub fn start(length: u64, now: u64) -> Self {
        Self::Running { start: now, end: now.saturating_add(length) }
    }

    pub fn pause(self, now: u64) -> Self {
        match self {
            Self::Running { start, end } => Self::Paused { left: end.saturating_sub(now), length: end - start },
            other => other,
        }
    }

    pub fn resume(self, now: u64) -> Self {
        match self {
            Self::Paused { left, length } => {
                let end = now.saturating_add(left);
                // The start moves too, so the ring stays in step.
                Self::Running { start: end.saturating_sub(length), end }
            }
            other => other,
        }
    }

    pub fn left(self, now: u64) -> u64 {
        match self {
            Self::Idle => 0,
            Self::Running { end, .. } => end.saturating_sub(now),
            Self::Paused { left, .. } => left,
        }
    }

    pub fn share_left(self, now: u64) -> f32 {
        let length = match self {
            Self::Idle => return 1.0,
            Self::Running { start, end } => end - start,
            Self::Paused { length, .. } => length,
        };
        if length == 0 { 0.0 } else { (self.left(now) as f64 / length as f64).clamp(0.0, 1.0) as f32 }
    }

    pub fn over(self, now: u64) -> bool {
        matches!(self, Self::Running { end, .. } if now >= end)
    }

    pub fn running(self) -> bool {
        matches!(self, Self::Running { .. })
    }

    /// Text like `running <start> <end>` or `paused <left> <length>`, empty when idle.
    pub fn save(self) -> String {
        match self {
            Self::Idle => String::new(),
            Self::Running { start, end } => format!("running {start} {end}"),
            Self::Paused { left, length } => format!("paused {left} {length}"),
        }
    }

    /// Reads back what `save` wrote. One that ran out or doesn't parse comes back idle.
    pub fn restore(saved: &str, now: u64) -> Self {
        let mut parts = saved.split_whitespace();
        let (Some(kind), Some(a), Some(b)) = (parts.next(), parts.next(), parts.next()) else { return Self::Idle };
        let (Ok(a), Ok(b)) = (a.parse::<u64>(), b.parse::<u64>()) else { return Self::Idle };
        match kind {
            "running" if b > now && b > a => Self::Running { start: a, end: b },
            "paused" if b > 0 && a <= b => Self::Paused { left: a, length: b },
            _ => Self::Idle,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTE: u64 = 60_000;

    #[test]
    fn resume_keeps_ring_in_step() {
        let running = Countdown::start(4 * MINUTE, 0);
        let paused = running.pause(MINUTE);
        assert_eq!(paused, Countdown::Paused { left: 3 * MINUTE, length: 4 * MINUTE });
        assert_eq!(paused.left(99 * MINUTE), 3 * MINUTE);
        let resumed = paused.resume(10 * MINUTE);
        assert_eq!(resumed, Countdown::Running { start: 9 * MINUTE, end: 13 * MINUTE });
        assert_eq!(resumed.share_left(10 * MINUTE), 0.75);
    }

    #[test]
    fn over_only_while_running() {
        let running = Countdown::start(MINUTE, 0);
        assert!(!running.over(MINUTE - 1) && running.over(MINUTE));
        assert!(!running.pause(MINUTE + 1).over(u64::MAX));
        assert!(!Countdown::Idle.over(u64::MAX));
    }

    #[test]
    fn save_round_trips() {
        let running = Countdown::start(4 * MINUTE, 1000);
        assert_eq!(Countdown::restore(&running.save(), 2000), running);
        let paused = running.pause(MINUTE);
        assert_eq!(Countdown::restore(&paused.save(), u64::MAX), paused);
        assert_eq!(Countdown::restore(&Countdown::Idle.save(), 0), Countdown::Idle);
    }

    #[test]
    fn stale_or_garbled_is_dropped() {
        let running = Countdown::start(MINUTE, 0);
        assert_eq!(Countdown::restore(&running.save(), MINUTE), Countdown::Idle);
        for junk in ["", "running", "running x y", "paused 5 0", "paused 9 3", "later 1 2"] {
            assert_eq!(Countdown::restore(junk, 0), Countdown::Idle, "{junk:?}");
        }
    }

    #[test]
    fn ring_drains_to_empty() {
        let running = Countdown::start(4 * MINUTE, 0);
        assert_eq!(running.share_left(0), 1.0);
        assert_eq!(running.share_left(MINUTE), 0.75);
        assert_eq!(running.share_left(9 * MINUTE), 0.0);
        assert_eq!(Countdown::Idle.share_left(5), 1.0);
    }
}
