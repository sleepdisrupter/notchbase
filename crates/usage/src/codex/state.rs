//! What the ChatGPT card shows. A failure never hides numbers already shown, and no logs at all reads as signed out.

use super::PlanUsage;
use super::scan::Cache;

#[derive(Debug, PartialEq)]
pub(super) enum Outcome {
    Ready(PlanUsage),
    Empty,
    Failed(String),
}

#[derive(Default)]
pub(super) struct Shared {
    pub(super) usage: PlanUsage,
    pub(super) version: u64,
    pub(super) in_flight: bool,
    pub(super) cache: Cache,
}

impl Shared {
    pub(super) fn begin(&mut self) -> bool {
        if self.in_flight {
            return false;
        }
        self.in_flight = true;
        true
    }

    pub(super) fn apply(&mut self, outcome: Outcome) {
        let next = match outcome {
            Outcome::Ready(usage) => usage,
            Outcome::Failed(_) if matches!(self.usage, PlanUsage::Ready { .. }) => return,
            Outcome::Failed(message) => PlanUsage::Error(message),
            Outcome::Empty => PlanUsage::SignedOut,
        };
        if next != self.usage {
            self.usage = next;
            self.version += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use super::*;

    fn ready(secs: u64) -> PlanUsage {
        PlanUsage::Ready { five_hour: None, weekly: None, fetched_at: UNIX_EPOCH + Duration::from_secs(secs) }
    }

    #[test]
    fn versions_move_only_on_change() {
        let mut shared = Shared::default();
        shared.apply(Outcome::Empty);
        assert_eq!((&shared.usage, shared.version), (&PlanUsage::SignedOut, 1));
        shared.apply(Outcome::Empty);
        assert_eq!(shared.version, 1);
        shared.apply(Outcome::Ready(ready(5)));
        shared.apply(Outcome::Ready(ready(5)));
        assert_eq!((&shared.usage, shared.version), (&ready(5), 2));
    }

    #[test]
    fn failures_never_hide_numbers() {
        let mut shared = Shared::default();
        shared.apply(Outcome::Failed("Couldn't read Codex usage".into()));
        assert_eq!(shared.usage, PlanUsage::Error("Couldn't read Codex usage".into()));
        shared.apply(Outcome::Ready(ready(5)));
        shared.apply(Outcome::Failed("x".into()));
        assert_eq!(shared.usage, ready(5));
        shared.apply(Outcome::Empty);
        assert_eq!(shared.usage, PlanUsage::SignedOut, "a removed Codex home is reported");
    }

    #[test]
    fn one_refresh_at_a_time() {
        let mut shared = Shared::default();
        assert!(shared.begin());
        assert!(!shared.begin());
        shared.in_flight = false;
        assert!(shared.begin());
    }
}
