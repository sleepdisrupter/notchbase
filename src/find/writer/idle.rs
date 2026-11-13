//! How long a quiet writer sleeps: until the next save, rebuild, reconcile or usage save is due. On battery it rarely
//! wakes sooner than every ten minutes.

use std::time::Duration;

use super::Writer;
use super::system::Moment;
use super::upkeep::{IDLE, IDLE_ROWS, SAVE_EVERY, SAVE_LATEST};

const BATTERY_RECHECK: Duration = Duration::from_secs(10 * 60);

impl Writer {
    pub(super) fn idle_wait(&self, now: Moment) -> Duration {
        let reconcile =
            self.reconcile_due(now).map(|due| if self.timers.on_battery() { due.max(BATTERY_RECHECK) } else { due });
        let deadlines = [self.save_due(now), self.rebuild_due(now), reconcile, self.learned_due(now)];
        deadlines.into_iter().flatten().min().unwrap_or(Duration::MAX)
    }

    fn save_due(&self, now: Moment) -> Option<Duration> {
        if !self.writes || !self.launched || !self.config.enabled {
            return None;
        }
        if self.shared.reading().revision() == self.timers.saved_revision {
            return None;
        }
        let since = now.stamp.saturating_sub(self.timers.saved_at);
        if since < SAVE_EVERY {
            return Some(seconds(SAVE_EVERY - since));
        }
        Some(if self.timers.on_battery() {
            seconds(SAVE_LATEST.saturating_sub(since)).min(BATTERY_RECHECK)
        } else {
            Duration::ZERO
        })
    }

    fn rebuild_due(&self, now: Moment) -> Option<Duration> {
        if self.timers.added <= IDLE_ROWS {
            return None;
        }
        let quiet = IDLE.saturating_sub(now.at.saturating_duration_since(self.timers.last_order));
        Some(if self.timers.on_battery() { quiet.max(BATTERY_RECHECK) } else { quiet })
    }

    fn learned_due(&self, now: Moment) -> Option<Duration> {
        let due = self.shared.usage.save_due().filter(|_| self.config.learn)?;
        Some(seconds(due.saturating_sub(now.stamp)))
    }
}

fn seconds(stamps: u32) -> Duration {
    Duration::from_secs(stamps.into())
}
