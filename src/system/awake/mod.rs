//! Keeps the computer awake while at least one holder asks for it, or while it is forced on.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use self::macos as imp;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
use self::windows as imp;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod other;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use self::other as imp;

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use imp::Assertion;

const RENEW_EVERY: Duration = Duration::from_secs(1);

#[cfg(target_os = "macos")]
pub fn request_lid_floor(disabled: bool, report: impl FnOnce(bool) + Send + 'static) {
    imp::request_lid_floor(disabled, report);
}

pub struct Awake<Holder: Ord> {
    holders: BTreeSet<Holder>,
    forced: bool,
    assertion: Option<Assertion>,
    renewed: Option<Instant>,
}

impl<Holder: Ord> Default for Awake<Holder> {
    fn default() -> Self {
        Self { holders: BTreeSet::new(), forced: false, assertion: None, renewed: None }
    }
}

impl<Holder: Ord> Awake<Holder> {
    pub fn set(&mut self, holder: Holder, awake: bool) -> bool {
        if awake {
            self.holders.insert(holder);
        } else {
            self.holders.remove(&holder);
        }
        self.follow()
    }

    pub fn force(&mut self, forced: bool) -> bool {
        self.forced = forced;
        self.follow()
    }

    pub fn is_forced(&self) -> bool {
        self.forced
    }

    pub fn is_awake(&self) -> bool {
        self.forced || !self.holders.is_empty()
    }

    pub fn renew(&mut self, now: Instant) {
        let started = self.renewed.is_none();
        if !self.renewal_due(now) {
            return;
        }
        match &mut self.assertion {
            Some(assertion) => assertion.renew(),
            None if started => imp::forget_lid(),
            None => {}
        }
    }

    fn renewal_due(&mut self, now: Instant) -> bool {
        let due = self.renewed.is_none_or(|at| now.saturating_duration_since(at) >= RENEW_EVERY);
        if due {
            self.renewed = Some(now);
        }
        due
    }

    pub fn release_all(&mut self) {
        self.holders.clear();
        self.follow();
    }

    fn follow(&mut self) -> bool {
        let wanted = self.forced || !self.holders.is_empty();
        if !wanted {
            self.assertion = None;
        } else if self.assertion.is_none() {
            self.assertion = Assertion::hold();
        }
        self.assertion.is_some() == wanted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "holds a real keep-awake assertion on this computer for a moment"]
    fn stays_awake_while_anyone_asks() {
        let mut awake = Awake::<u8>::default();
        assert!(awake.set(1, true));
        assert!(awake.set(2, true));
        assert!(awake.set(1, false));
        assert!(awake.assertion.is_some(), "the second holder still wants it awake");
        assert!(awake.set(2, false));
        assert!(awake.assertion.is_none());
    }

    #[test]
    fn is_awake_follows_holders_and_force() {
        let mut awake = Awake::<u8>::default();
        assert!(!awake.is_awake(), "nothing holds it at rest");
        awake.set(1, true);
        assert!(awake.is_awake(), "a holder makes it awake");
        awake.set(1, false);
        assert!(!awake.is_awake(), "the last holder leaving ends it");
        awake.force(true);
        assert!(awake.is_awake(), "forcing it awake counts with no holders");
    }

    #[test]
    fn forced_ignores_letting_go() {
        let mut awake = Awake::<u8> { forced: true, ..Default::default() };
        awake.release_all();
        assert!(awake.forced && awake.holders.is_empty());
        awake.set(1, false);
        assert!(awake.forced);
    }

    #[test]
    fn hold_renews_once_a_second() {
        let mut awake = Awake::<u8>::default();
        let start = Instant::now();
        assert!(awake.renewal_due(start), "the first look is always due");
        assert!(!awake.renewal_due(start + RENEW_EVERY / 2));
        assert!(awake.renewal_due(start + RENEW_EVERY));
        assert!(!awake.renewal_due(start + RENEW_EVERY));
    }

    #[test]
    fn letting_go_without_holding_is_fine() {
        let mut awake = Awake::<u8>::default();
        assert!(awake.set(7, false));
        awake.release_all();
        assert!(awake.assertion.is_none());
    }
}
