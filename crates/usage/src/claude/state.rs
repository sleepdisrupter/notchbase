//! What the Claude card shows and how failures are held off. Errors never replace numbers less than 30 minutes old,
//! and a Retry-After or a Keychain refusal pauses automatic requests.

use std::time::{Duration, Instant};

use super::PlanUsage;
use super::credentials::Credentials;
use super::desktop;

const KEEP_STALE: Duration = Duration::from_secs(30 * 60);
const LONGEST_HOLD: Duration = Duration::from_secs(3600);
const KEYCHAIN_DENIED: &str = "Allow Keychain access to read Claude usage";

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) enum Hold {
    #[default]
    None,
    Until(Instant),
    UntilAsked,
}

#[derive(Debug, PartialEq)]
pub(super) enum Outcome {
    Ready(PlanUsage),
    Stale(PlanUsage),
    Unchanged,
    SignedOut,
    Expired,
    KeychainDenied,
    Failed { message: String, retry_after: Option<Duration> },
}

impl Outcome {
    pub(super) fn failed(message: impl Into<String>) -> Self {
        Outcome::Failed { message: message.into(), retry_after: None }
    }
}

#[derive(Default)]
pub(super) struct Shared {
    pub(super) usage: PlanUsage,
    pub(super) version: u64,
    pub(super) in_flight: bool,
    pub(super) hold: Hold,
    pub(super) last_api: Option<Instant>,
    pub(super) cached: Option<Credentials>,
    pub(super) history: Option<desktop::Cache>,
}

impl Shared {
    pub(super) fn begin(&mut self, manual: bool) -> bool {
        if self.in_flight {
            return false;
        }
        if manual {
            self.hold = Hold::None;
        }
        self.in_flight = true;
        true
    }

    pub(super) fn note_hold(&mut self, outcome: &Outcome, now: Instant) {
        match outcome {
            Outcome::KeychainDenied => self.hold = Hold::UntilAsked,
            Outcome::Failed { retry_after: Some(wait), .. } => self.hold = Hold::Until(now + (*wait).min(LONGEST_HOLD)),
            _ => {}
        }
    }

    pub(super) fn apply(&mut self, outcome: Outcome, now: Instant) {
        self.note_hold(&outcome, now);
        let next = match outcome {
            Outcome::Unchanged => return,
            Outcome::Ready(usage) => usage,
            Outcome::Stale(usage) if self.shows_newer_than(&usage) => return,
            Outcome::Stale(usage) => usage,
            _ if self.has_recent_numbers() => return,
            Outcome::SignedOut => PlanUsage::SignedOut,
            Outcome::Expired => PlanUsage::Expired,
            Outcome::KeychainDenied => PlanUsage::Error(KEYCHAIN_DENIED.to_owned()),
            Outcome::Failed { message, .. } => PlanUsage::Error(message),
        };
        if next != self.usage {
            self.usage = next;
            self.version += 1;
        }
    }

    fn shows_newer_than(&self, stale: &PlanUsage) -> bool {
        matches!(
            (&self.usage, stale),
            (PlanUsage::Ready { fetched_at: current, .. }, PlanUsage::Ready { fetched_at: stale, .. })
                if current > stale
        )
    }

    fn has_recent_numbers(&self) -> bool {
        matches!(&self.usage, PlanUsage::Ready { fetched_at, .. }
            if fetched_at.elapsed().map_or(true, |age| age < KEEP_STALE))
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn ready_at(fetched_at: SystemTime) -> PlanUsage {
        PlanUsage::Ready { five_hour: None, weekly: None, fetched_at }
    }

    fn ready(secs: u64) -> PlanUsage {
        ready_at(UNIX_EPOCH + Duration::from_secs(secs))
    }

    #[test]
    fn failures_keep_recent_data() {
        let mut shared = Shared::default();
        let now = Instant::now();
        shared.apply(Outcome::failed("Can't reach Anthropic"), now);
        assert_eq!(shared.usage, PlanUsage::Error("Can't reach Anthropic".into()));

        let recent = ready_at(SystemTime::now());
        shared.apply(Outcome::Ready(recent.clone()), now);
        let version = shared.version;
        shared.apply(
            Outcome::Failed {
                message: "Rate limited, try again later".into(),
                retry_after: Some(Duration::from_secs(90)),
            },
            now,
        );
        assert_eq!(shared.usage, recent, "recent data survives a failed refresh");
        assert_eq!(shared.version, version);
        assert_eq!(shared.hold, Hold::Until(now + Duration::from_secs(90)));

        shared.apply(Outcome::Expired, now);
        assert_eq!(shared.usage, recent);
        shared.apply(Outcome::KeychainDenied, now);
        assert_eq!(shared.usage, recent);
        assert_eq!(shared.hold, Hold::UntilAsked);

        shared.apply(Outcome::Ready(ready_at(SystemTime::now() - Duration::from_secs(3600))), now);
        shared.apply(Outcome::Expired, now);
        assert_eq!(shared.usage, PlanUsage::Expired, "old numbers give way to the reason");
        shared.apply(Outcome::KeychainDenied, now);
        assert_eq!(shared.usage, PlanUsage::Error(KEYCHAIN_DENIED.into()));
    }

    #[test]
    fn stale_never_replaces_newer() {
        let now = Instant::now();
        let mut shared = Shared::default();
        shared.apply(Outcome::Stale(ready(100)), now);
        assert_eq!(shared.usage, ready(100));
        shared.apply(Outcome::Expired, now);
        shared.apply(Outcome::Stale(ready(100)), now);
        assert_eq!(shared.usage, ready(100));
        shared.apply(Outcome::Ready(ready(500)), now);
        shared.apply(Outcome::Stale(ready(100)), now);
        assert_eq!(shared.usage, ready(500));
        shared.apply(Outcome::Stale(ready(600)), now);
        assert_eq!(shared.usage, ready(600));
        let version = shared.version;
        shared.apply(Outcome::Unchanged, now);
        assert_eq!((shared.usage.clone(), shared.version), (ready(600), version));
    }

    #[test]
    fn holds_are_capped_and_manual_clears() {
        let now = Instant::now();
        let mut shared = Shared::default();
        shared.note_hold(
            &Outcome::Failed { message: String::new(), retry_after: Some(Duration::from_secs(86_400)) },
            now,
        );
        assert_eq!(shared.hold, Hold::Until(now + LONGEST_HOLD));
        assert!(shared.begin(false));
        assert!(!shared.begin(true), "only one refresh runs at a time");
        assert_eq!(shared.hold, Hold::Until(now + LONGEST_HOLD));
        shared.in_flight = false;
        assert!(shared.begin(true));
        assert_eq!(shared.hold, Hold::None);
    }
}
