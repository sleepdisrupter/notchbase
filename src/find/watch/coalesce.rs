//! Lets changes settle before the writer acts on them. Most wait a quarter of a second, changes in noisy folders such
//! as node_modules wait in a slow lane for five seconds (ten on battery), and anything outside the plan is dropped.

use std::time::{Duration, Instant};

use crate::find::scan::Plan;

use super::lane::Lane;
use super::{Change, Event};

const SETTLE: Duration = Duration::from_millis(250);
const SLOW_EVERY: Duration = Duration::from_secs(5);
const SLOW_EVERY_ON_BATTERY: Duration = Duration::from_secs(10);

#[derive(Default)]
pub struct Coalescer {
    settling: Lane,
    slow: Lane,
    slow_since: Option<Instant>,
}

impl Coalescer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, event: Event, plan: &Plan, now: Instant) {
        let Some(change) = admitted(event.change, plan) else { return };
        if noisy(&change, plan) {
            self.slow.hold(change, event.id, now);
            self.slow_since.get_or_insert(now);
        } else {
            self.settling.hold(change, event.id, now);
        }
    }

    pub fn due(&mut self, now: Instant, on_battery: bool) -> Vec<Event> {
        let mut out = self.settling.take(|held| now.saturating_duration_since(held.since) >= SETTLE);
        let every = if on_battery { SLOW_EVERY_ON_BATTERY } else { SLOW_EVERY };
        if self.slow_since.is_some_and(|since| now.saturating_duration_since(since) >= every) {
            out.extend(self.slow.take(|_| true));
            self.slow_since = None;
        }
        out.sort_by_key(|event| event.id);
        out
    }

    pub fn is_empty(&self) -> bool {
        self.settling.is_empty() && self.slow.is_empty()
    }

    pub fn oldest(&self) -> Option<u64> {
        self.settling.oldest().into_iter().chain(self.slow.oldest()).min()
    }
}

pub(in crate::find) fn admitted(change: Change, plan: &Plan) -> Option<Change> {
    match change {
        Change::Touched(ref path) | Change::Relist(ref path) | Change::Subtree(ref path) => {
            plan.wanted(path).then_some(change)
        }
        Change::Moved { from, to } => match (plan.wanted(&from), plan.wanted(&to)) {
            (true, true) => Some(Change::Moved { from, to }),
            (true, false) => Some(Change::Touched(from)),
            (false, true) => Some(Change::Touched(to)),
            (false, false) => None,
        },
        Change::Overflow(_) | Change::Everything(_) => Some(change),
    }
}

fn noisy(change: &Change, plan: &Plan) -> bool {
    match change {
        Change::Touched(path) | Change::Relist(path) | Change::Subtree(path) => plan.noisy(path),
        Change::Moved { from, to } => plan.noisy(from) || plan.noisy(to),
        Change::Overflow(_) | Change::Everything(_) => false,
    }
}

#[cfg(test)]
mod tests;
