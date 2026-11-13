//! How hard the scan works. On mains or with Find open it runs four listers, on battery two that rest a fifth of the
//! time, and under 20% battery it pauses.

use std::time::Duration;

use notchbase_sdk::protocol::Power;

const LOW_BATTERY: u8 = 20;
const FULL_WORKERS: usize = 4;
const GENTLE_WORKERS: usize = 2;
const WORK_PER_REST: u32 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pace {
    Full,
    Gentle,
    Paused,
}

impl Pace {
    pub const MOST_WORKERS: usize = FULL_WORKERS;

    pub fn of(power: &Power, visible: bool) -> Self {
        match power.level {
            _ if power.plugged => Self::Full,
            Some(level) if level < LOW_BATTERY => Self::Paused,
            Some(_) if !visible => Self::Gentle,
            _ => Self::Full,
        }
    }

    pub(super) fn workers(self) -> usize {
        match self {
            Self::Full => FULL_WORKERS,
            Self::Gentle => GENTLE_WORKERS,
            Self::Paused => 0,
        }
    }

    pub(super) fn rest(self, worked: Duration) -> Duration {
        if self == Self::Gentle { worked / WORK_PER_REST } else { Duration::ZERO }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn power(level: Option<u8>, plugged: bool) -> Power {
        Power { level, plugged, charging: plugged }
    }

    #[test]
    fn full_pace_on_mains_or_open() {
        assert_eq!(Pace::of(&power(Some(5), true), false), Pace::Full);
        assert_eq!(Pace::of(&power(None, false), false), Pace::Full, "a desktop has no battery to spare");
        assert_eq!(Pace::of(&power(Some(60), false), true), Pace::Full, "someone is waiting for results");
        assert_eq!(Pace::of(&power(Some(20), false), true), Pace::Full);
    }

    #[test]
    fn battery_slows_and_low_pauses() {
        assert_eq!(Pace::of(&power(Some(60), false), false), Pace::Gentle);
        assert_eq!(Pace::of(&power(Some(20), false), false), Pace::Gentle);
        assert_eq!(Pace::of(&power(Some(19), false), false), Pace::Paused);
        assert_eq!(Pace::of(&power(Some(19), false), true), Pace::Paused);
    }

    #[test]
    fn gentle_rests_a_fifth() {
        let worked = Duration::from_millis(80);
        assert_eq!(Pace::Gentle.rest(worked), Duration::from_millis(20));
        assert_eq!((Pace::Full.rest(worked), Pace::Paused.rest(worked)), (Duration::ZERO, Duration::ZERO));
        assert_eq!((Pace::Full.workers(), Pace::Gentle.workers(), Pace::Paused.workers()), (4, 2, 0));
    }
}
