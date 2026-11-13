//! The countdown behind a focus session or a break.

use notchbase_sdk::prelude::Tone;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Focus,
    Break,
}

impl Phase {
    pub fn title(self) -> &'static str {
        match self {
            Phase::Idle => "Ready",
            Phase::Focus => "Focus",
            Phase::Break => "Break",
        }
    }

    pub fn tone(self) -> Tone {
        match self {
            Phase::Idle => Tone::Secondary,
            Phase::Focus => Tone::Orange,
            Phase::Break => Tone::Green,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Run {
    Running { end: u64 },
    Paused { left: u64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Countdown {
    phase: Phase,
    length: u64,
    run: Run,
}

impl Countdown {
    pub fn idle(length: u64) -> Self {
        Self { phase: Phase::Idle, length, run: Run::Paused { left: length } }
    }

    pub fn start(phase: Phase, length: u64, now: u64) -> Self {
        Self { phase, length, run: Run::Running { end: now.saturating_add(length) } }
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn run(&self) -> Run {
        self.run
    }

    pub fn end(&self) -> Option<u64> {
        match self.run {
            Run::Running { end } => Some(end),
            Run::Paused { .. } => None,
        }
    }

    pub fn running(&self) -> bool {
        self.end().is_some()
    }

    pub fn toggle(&mut self, now: u64) {
        *self = match (self.phase, self.run) {
            (Phase::Idle, _) => Self::start(Phase::Focus, self.length, now),
            (_, Run::Running { end }) => Self { run: Run::Paused { left: end.saturating_sub(now) }, ..*self },
            (_, Run::Paused { left }) => Self { run: Run::Running { end: now.saturating_add(left) }, ..*self },
        };
    }

    pub fn left(&self, now: u64) -> u64 {
        match self.run {
            Run::Running { end } => end.saturating_sub(now),
            Run::Paused { left } => left,
        }
    }

    pub fn progress(&self, now: u64) -> f32 {
        if self.length == 0 {
            return 0.0;
        }
        (1.0 - self.left(now) as f64 / self.length as f64).clamp(0.0, 1.0) as f32
    }

    pub fn over(&self, now: u64) -> bool {
        self.end().is_some_and(|end| now >= end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTE: u64 = 60_000;

    #[test]
    fn idle_waits_at_full_length() {
        let idle = Countdown::idle(25 * MINUTE);
        assert_eq!((idle.phase(), idle.end(), idle.running()), (Phase::Idle, None, false));
        assert_eq!(idle.left(999_999), 25 * MINUTE);
        assert_eq!(idle.progress(999_999), 0.0);
        assert!(!idle.over(u64::MAX));
    }

    #[test]
    fn toggle_starts_focus_now() {
        let mut countdown = Countdown::idle(25 * MINUTE);
        countdown.toggle(1_000);
        assert_eq!(countdown.phase(), Phase::Focus);
        assert_eq!(countdown.run(), Run::Running { end: 1_000 + 25 * MINUTE });
    }

    #[test]
    fn pause_keeps_what_is_left() {
        let mut countdown = Countdown::start(Phase::Focus, 25 * MINUTE, 0);
        countdown.toggle(10 * MINUTE);
        assert_eq!(countdown.run(), Run::Paused { left: 15 * MINUTE });
        assert_eq!(countdown.left(60 * MINUTE), 15 * MINUTE, "time stands still while paused");
        countdown.toggle(60 * MINUTE);
        assert_eq!(countdown.run(), Run::Running { end: 75 * MINUTE });
        assert_eq!(countdown.phase(), Phase::Focus);
    }

    #[test]
    fn progress_fills_up_as_time_runs_out() {
        let countdown = Countdown::start(Phase::Break, 4 * MINUTE, 0);
        assert_eq!(countdown.progress(0), 0.0);
        assert_eq!(countdown.progress(MINUTE), 0.25);
        assert_eq!(countdown.progress(4 * MINUTE), 1.0);
        assert_eq!(countdown.progress(9 * MINUTE), 1.0);
    }

    #[test]
    fn over_only_while_running() {
        let mut countdown = Countdown::start(Phase::Focus, MINUTE, 0);
        assert!(!countdown.over(MINUTE - 1));
        assert!(countdown.over(MINUTE));
        countdown.toggle(MINUTE + 5);
        assert_eq!(countdown.run(), Run::Paused { left: 0 });
        assert!(!countdown.over(u64::MAX));
    }

    #[test]
    fn extreme_clocks_saturate() {
        let mut countdown = Countdown::start(Phase::Focus, MINUTE, u64::MAX - 1);
        assert_eq!(countdown.end(), Some(u64::MAX));
        countdown.toggle(0);
        assert_eq!(countdown.left(0), u64::MAX);
        assert_eq!(Countdown::idle(0).progress(0), 0.0);
    }

    #[test]
    fn phases_have_titles_and_tones() {
        assert_eq!([Phase::Idle, Phase::Focus, Phase::Break].map(Phase::title), ["Ready", "Focus", "Break"]);
        assert_eq!(Phase::Focus.tone(), Tone::Orange);
        assert_eq!(Phase::Break.tone(), Tone::Green);
    }
}
