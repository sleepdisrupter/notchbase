//! A local log of plan usage for the week view, kept in the data directory. It never leaves the computer.

mod sample;
mod series;
mod store;
mod week;
mod writer;

use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::time::SystemTime;

pub use week::{DAYS, Day, UsageHistory, Week};

use super::{PlanUsage, Wake, lock};
use crate::assistant::Provider;
use crate::platform;
use sample::Sample;
use series::Series;
use store::{COMPACT_TO, MAX_LINES};
use week::{next_midnight_ms, unix_ms};
use writer::Loaded;

pub(super) struct History {
    series: Series,
    loaded: Loaded,
    writer: Option<Sender<Sample>>,
    weeks: UsageHistory,
    pinned: bool,
    stale: bool,
    next_day_ms: u64,
}

impl History {
    pub(super) fn open(wake: Wake, writes: bool) -> Self {
        let loaded = Loaded::default();
        let writer = writer::spawn(store::path(), writes, Arc::clone(&loaded), wake);
        Self::with(loaded, writer)
    }

    fn with(loaded: Loaded, writer: Option<Sender<Sample>>) -> Self {
        Self {
            series: Series::default(),
            loaded,
            writer,
            weeks: UsageHistory::default(),
            pinned: false,
            stale: true,
            next_day_ms: 0,
        }
    }

    /// Logs a fresh reading. A repeat, or anything while a preview pins the history, is dropped.
    pub(super) fn observe(&mut self, provider: Provider, usage: &PlanUsage) {
        let Some(sample) = Sample::of(provider, usage).filter(|_| !self.pinned) else { return };
        if !self.series.push(sample) {
            return;
        }
        self.stale = true;
        if self.series.len() > MAX_LINES {
            self.series.prune(sample.at_ms);
            self.series.thin(COMPACT_TO);
        }
        if self.writer.as_ref().is_some_and(|writer| writer.send(sample).is_err()) {
            self.writer = None;
        }
    }

    pub(super) fn pin(&mut self, weeks: UsageHistory) {
        self.pinned = true;
        self.writer = None;
        self.weeks = weeks;
    }

    /// Rebuilds the weeks when a sample came in, the file loaded or midnight passed. True when they changed.
    pub(super) fn sync(&mut self, now: SystemTime) -> bool {
        if self.pinned {
            return false;
        }
        if let Some(mut loaded) = lock(&self.loaded).take() {
            // Readings taken while the file loaded go on top of it.
            for sample in self.series.samples() {
                loaded.push(*sample);
            }
            self.series = loaded;
            self.stale = true;
        }
        let now_ms = unix_ms(now);
        if !self.stale && now_ms < self.next_day_ms {
            return false;
        }
        let offset = platform::utc_offset_secs(platform::local_time(), now);
        let weeks = UsageHistory::of(self.series.samples(), now_ms, offset);
        self.next_day_ms = next_midnight_ms(now_ms, offset);
        self.stale = false;
        let changed = weeks != self.weeks;
        self.weeks = weeks;
        changed
    }

    pub(super) fn weeks(&self) -> &UsageHistory {
        &self.weeks
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::stats::UsageWindow;

    fn ready(five_hour: f32, fetched_at: SystemTime) -> PlanUsage {
        let window = UsageWindow { used: five_hour, resets_at: None, estimated: false, minutes: Some(300) };
        PlanUsage::Ready { five_hour: Some(window), weekly: None, fetched_at }
    }

    fn today(history: &History, provider: Provider) -> Option<f32> {
        history.weeks().week(provider).days[DAYS - 1].peak
    }

    fn local_noon() -> SystemTime {
        let now = SystemTime::now();
        let offset = platform::utc_offset_secs(platform::local_time(), now);
        let noon_ms = next_midnight_ms(unix_ms(now), offset) - 12 * 60 * 60 * 1000;
        SystemTime::UNIX_EPOCH + Duration::from_millis(noon_ms)
    }

    #[test]
    fn fetches_are_logged_once_and_merged() {
        let loaded = Loaded::default();
        let (writer, written) = std::sync::mpsc::channel();
        let mut history = History::with(Arc::clone(&loaded), Some(writer));
        let now = local_noon();
        let usage = ready(0.3, now);
        history.observe(Provider::Claude, &usage);
        history.observe(Provider::Claude, &usage);
        history.observe(Provider::Claude, &PlanUsage::Expired);
        assert_eq!(written.try_iter().count(), 1, "only a new fetch is logged");
        assert!(history.sync(now));
        assert_eq!(today(&history, Provider::Claude), Some(0.3));
        assert!(!history.sync(now), "nothing new, nothing recomputed");

        let earlier = unix_ms(now - Duration::from_secs(60));
        let from_file = Sample { at_ms: earlier, provider: Provider::Codex, five_hour: Some(0.7), weekly: None };
        *lock(&loaded) = Some(Series::new(vec![from_file]));
        assert!(history.sync(now));
        assert_eq!(today(&history, Provider::Codex), Some(0.7));
        assert_eq!(today(&history, Provider::Claude), Some(0.3), "samples logged before the load survive it");
    }

    #[test]
    fn pinned_history_ignores_fetches() {
        let (writer, written) = std::sync::mpsc::channel();
        let mut history = History::with(Loaded::default(), Some(writer));
        let pinned = UsageHistory::both(Week::ending_today([Some(0.5); DAYS]));
        history.pin(pinned);
        history.observe(Provider::Claude, &ready(0.9, SystemTime::now()));
        assert!(!history.sync(SystemTime::now()));
        assert_eq!(history.weeks(), &pinned);
        assert_eq!(written.try_iter().count(), 0);
    }
}
