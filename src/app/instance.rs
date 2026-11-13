//! Only one notchbase runs at a time. A newer copy can take over from an older one.

use std::time::Duration;

use crate::installer::Arrival;
use crate::platform::{self, InstanceLock};

use super::App;
use super::preview::previewing;

const HANDOVER_TRIES: u32 = 20;
const HANDOVER_PAUSE: Duration = Duration::from_millis(250);
const ALREADY_RUNNING: &str = "notchbase is already running";

pub fn claim_instance(arrival: Arrival) -> Option<InstanceLock> {
    if previewing() {
        return Some(InstanceLock::unguarded());
    }
    claim_within(tries_for(arrival), HANDOVER_PAUSE, platform::claim_instance)
}

pub fn step_aside() {
    let shown = crate::assistant::show_running();
    let how = if shown { "showing it" } else { "leaving it be" };
    eprintln!("notchbase: {ALREADY_RUNNING}, {how}");
}

fn tries_for(arrival: Arrival) -> u32 {
    match arrival {
        Arrival::Fresh => 1,
        Arrival::Updated | Arrival::Installed => HANDOVER_TRIES,
    }
}

fn claim_within<L>(tries: u32, pause: Duration, mut claim: impl FnMut() -> Option<L>) -> Option<L> {
    for attempt in 0..tries {
        if attempt > 0 {
            std::thread::sleep(pause);
        }
        if let Some(lock) = claim() {
            return Some(lock);
        }
    }
    None
}

impl App {
    pub(super) fn sync_shows(&mut self) {
        let shows = self.inbox.shows();
        if shows == self.shows_seen {
            return;
        }
        self.shows_seen = shows;
        self.notify(ALREADY_RUNNING);
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::time::Instant;

    use super::*;

    #[test]
    fn fresh_tries_once_handover_retries() {
        assert_eq!(tries_for(Arrival::Fresh), 1);
        assert_eq!(tries_for(Arrival::Updated), HANDOVER_TRIES);
        assert_eq!(tries_for(Arrival::Installed), HANDOVER_TRIES);
        assert!(HANDOVER_PAUSE * (HANDOVER_TRIES - 1) <= Duration::from_secs(5));
    }

    #[test]
    fn retries_until_released() {
        let calls = Cell::new(0);
        let claimed = claim_within(20, Duration::ZERO, || {
            calls.set(calls.get() + 1);
            (calls.get() == 4).then_some("lock")
        });
        assert_eq!((claimed, calls.get()), (Some("lock"), 4));
    }

    #[test]
    fn gives_up_after_last_try() {
        let calls = Cell::new(0);
        let started = Instant::now();
        let claimed: Option<()> = claim_within(3, Duration::from_millis(10), || {
            calls.set(calls.get() + 1);
            None
        });
        assert_eq!((claimed, calls.get()), (None, 3));
        assert!(started.elapsed() >= Duration::from_millis(20));
    }

    #[test]
    fn fresh_start_never_waits() {
        let calls = Cell::new(0);
        let claimed: Option<()> = claim_within(tries_for(Arrival::Fresh), Duration::from_secs(60), || {
            calls.set(calls.get() + 1);
            None
        });
        assert_eq!((claimed, calls.get()), (None, 1));
    }

    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    fn handover_waits_for_release() {
        let dir = std::env::temp_dir().join(format!("notchbase-handover-{}", std::process::id()));
        let mut old = Some(platform::claim_instance_in(&dir).expect("the old copy's lock"));
        let tries = Cell::new(0);
        let new = claim_within(HANDOVER_TRIES, Duration::ZERO, || {
            tries.set(tries.get() + 1);
            if tries.get() == 3 {
                old.take();
            }
            platform::claim_instance_in(&dir)
        });
        assert!(new.is_some(), "the new copy takes over");
        assert_eq!(tries.get(), 3, "the first two tries find the old copy still running");
    }
}
