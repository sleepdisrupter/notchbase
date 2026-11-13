//! Reconciling: rescanning a finished root now and then to catch what the watcher missed. A light pass comes after a
//! week and a full sweep after a month, or every day without a saved cursor, and an unwatched root gets a pass every
//! half hour. It waits until Find is hidden and quiet, and on battery only unwatched roots get a light pass.

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::Duration;

use crate::find::context::{DAY, HOUR, MINUTE, Stamp};
use crate::find::store::{RootMark, mark};

use super::changes::{EVERYTHING, RECONCILE_REACH};
use super::errands::{Duty, Purpose};
use super::system::Moment;
use super::{Work, Writer};

const QUIET: Duration = Duration::from_secs(60);
const RECONCILE_AFTER: Stamp = 7 * DAY;
const SWEEP_AFTER: Stamp = 30 * DAY;
const SWEEP_NO_CURSOR: Stamp = 24 * HOUR;
const RECONCILE_UNWATCHED: Stamp = 30 * MINUTE;

impl Writer {
    pub(super) fn reconcile(&mut self, now: Moment) {
        if !self.launched || self.errands.any(Purpose::Reconcile) {
            return;
        }
        if let Some(root) = self.reconciles.pop() {
            return self.reconcile_root(root, RECONCILE_REACH, now);
        }
        let visible = self.shared.visible.load(Ordering::Acquire);
        let quiet = now.at.saturating_duration_since(self.timers.last_order) >= QUIET;
        if visible || !quiet {
            return;
        }
        let battery = self.timers.on_battery();
        let complete = self.meta.roots.iter().filter(|found| found.flags & mark::COMPLETE != 0);
        let allowed = complete.filter(|found| !battery || self.unwatched.contains(&found.path));
        let due = allowed.filter(|found| self.plan.wanted(&found.path)).find_map(|found| {
            let reach = self.due_reach(found, now.stamp)?;
            Some((found.path.clone(), if battery { RECONCILE_REACH } else { reach }))
        });
        if let Some((root, reach)) = due {
            self.reconcile_root(root, reach, now);
        }
    }

    fn reconcile_root(&mut self, root: PathBuf, reach: u8, now: Moment) {
        self.probe(now);
        if !self.plan.wanted(&root) {
            return;
        }
        let duties = vec![Duty::Scanned { root: root.clone(), reach }];
        self.errand(None, duties, Purpose::Reconcile, vec![Work { dir: root, deep: true, reach }], now);
        self.steer();
    }

    pub(super) fn reconcile_due(&self, now: Moment) -> Option<Duration> {
        if !self.launched {
            return None;
        }
        let complete = self.meta.roots.iter().filter(|found| found.flags & mark::COMPLETE != 0);
        let at = complete.filter(|found| self.plan.wanted(&found.path)).map(|found| self.due_at(found)).min()?;
        let waiting = Duration::from_secs(at.saturating_sub(now.stamp).into());
        Some(waiting.max(QUIET.saturating_sub(now.at.saturating_duration_since(self.timers.last_order))))
    }

    fn due_at(&self, found: &RootMark) -> Stamp {
        let unwatched = self.unwatched.contains(&found.path);
        let cursor = self.system.keeps_cursor() && !unwatched;
        let sweep_after = if cursor { SWEEP_AFTER } else { SWEEP_NO_CURSOR };
        let swept = found.swept_at.saturating_add(sweep_after);
        let reconcile_after = if unwatched { RECONCILE_UNWATCHED } else { RECONCILE_AFTER };
        let reconciled = found.scanned_at.saturating_add(reconcile_after);
        if unwatched || cursor { swept.min(reconciled) } else { swept }
    }

    fn due_reach(&self, found: &RootMark, now: Stamp) -> Option<u8> {
        let unwatched = self.unwatched.contains(&found.path);
        let cursor = self.system.keeps_cursor() && !unwatched;
        let sweep_after = if cursor { SWEEP_AFTER } else { SWEEP_NO_CURSOR };
        if now.saturating_sub(found.swept_at) >= sweep_after {
            return Some(EVERYTHING);
        }
        let reconcile_after = match (unwatched, cursor) {
            (true, _) => Some(RECONCILE_UNWATCHED),
            (false, true) => Some(RECONCILE_AFTER),
            (false, false) => None,
        };
        reconcile_after.filter(|after| now.saturating_sub(found.scanned_at) >= *after).map(|_| RECONCILE_REACH)
    }
}
