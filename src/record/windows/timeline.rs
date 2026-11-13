//! Clocks for a recording in Media Foundation's 100 ns ticks: video frames on a fixed grid from the start, and audio
//! counted in samples for each source.

use std::time::{Duration, Instant};

/// Media Foundation times are in 100 ns ticks.
pub(super) const TICKS_PER_SECOND: u64 = 10_000_000;
/// A packet up to this far ahead of what's written is jitter, and gets no silence in front.
const EARLY_TOLERANCE_MS: u64 = 30;
/// A lane may lag the wall clock this much before it's filled with silence.
const LATE_ALLOWANCE_MS: u64 = 250;

pub(super) struct Frames {
    start: Instant,
    fps: u64,
    next: u64,
}

impl Frames {
    pub(super) fn new(start: Instant, fps: u32) -> Self {
        Self { start, fps: u64::from(fps.max(1)), next: 0 }
    }

    pub(super) fn deadline(&self) -> Instant {
        self.start + Duration::from_nanos(self.next * 1_000_000_000 / self.fps)
    }

    /// The frame due now, if any. A late frame jumps to the clock's index, so video skips rather than drifts.
    pub(super) fn due(&mut self, now: Instant) -> Option<u64> {
        if now < self.deadline() {
            return None;
        }
        let index = now.duration_since(self.start).as_nanos() as u64 * self.fps / 1_000_000_000;
        let index = index.max(self.next);
        self.next = index + 1;
        Some(index)
    }

    pub(super) fn time(&self, index: u64) -> i64 {
        (index * TICKS_PER_SECOND / self.fps) as i64
    }

    pub(super) fn end(&self) -> i64 {
        self.time(self.next)
    }
}

pub(super) struct Samples {
    rate: u64,
    written: u64,
}

impl Samples {
    pub(super) fn new(rate: u32) -> Self {
        Self { rate: u64::from(rate), written: 0 }
    }

    pub(super) fn written(&self) -> u64 {
        self.written
    }

    pub(super) fn time(&self) -> i64 {
        self.time_of(self.written)
    }

    pub(super) fn time_of(&self, frames: u64) -> i64 {
        (u128::from(frames) * u128::from(TICKS_PER_SECOND) / u128::from(self.rate)) as i64
    }

    pub(super) fn frames_in(&self, ticks: i64) -> u64 {
        (u128::from(ticks.max(0) as u64) * u128::from(self.rate) / u128::from(TICKS_PER_SECOND)) as u64
    }

    pub(super) fn gap_before(&self, starts_at: Option<i64>) -> u64 {
        let Some(start) = starts_at.map(|ticks| self.frames_in(ticks)) else { return 0 };
        if start > self.written + self.rate * EARLY_TOLERANCE_MS / 1000 { start - self.written } else { 0 }
    }

    pub(super) fn behind(&self, elapsed: i64) -> u64 {
        self.frames_in(elapsed).saturating_sub(self.rate * LATE_ALLOWANCE_MS / 1000).saturating_sub(self.written)
    }

    pub(super) fn short_of(&self, end: i64) -> u64 {
        self.frames_in(end).saturating_sub(self.written)
    }

    pub(super) fn advance(&mut self, frames: u64) {
        self.written += frames;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_follow_clock_and_skip_missed() {
        let start = Instant::now();
        let mut frames = Frames::new(start, 60);
        assert_eq!(frames.due(start), Some(0));
        assert_eq!(frames.due(start + Duration::from_millis(10)), None);
        assert_eq!(frames.deadline(), start + Duration::from_nanos(16_666_666));
        assert_eq!(frames.due(start + Duration::from_millis(17)), Some(1));
        assert_eq!(frames.due(start + Duration::from_millis(100)), Some(6));
        assert_eq!(frames.time(6), 1_000_000);
        assert_eq!(frames.end(), frames.time(7));
        assert_eq!(Frames::new(start, 30).time(3), 1_000_000);
    }

    #[test]
    fn late_packets_get_silence_not_jitter() {
        let mut samples = Samples::new(48_000);
        assert_eq!(samples.gap_before(Some(100_000)), 0);
        assert_eq!(samples.gap_before(Some(1_000_000)), 4_800);
        assert_eq!(samples.gap_before(None), 0);
        samples.advance(4_800);
        assert_eq!(samples.time(), 1_000_000);
        assert_eq!(samples.gap_before(Some(1_200_000)), 0);
        assert_eq!(samples.gap_before(Some(-5_000_000)), 0);
    }

    #[test]
    fn quiet_gaps_filled_after_allowance() {
        let mut samples = Samples::new(48_000);
        assert_eq!(samples.behind(2_000_000), 0);
        assert_eq!(samples.behind(10_000_000), 36_000);
        samples.advance(36_000);
        assert_eq!(samples.behind(10_000_000), 0);
        assert_eq!(samples.short_of(10_000_000), 12_000);
    }
}
