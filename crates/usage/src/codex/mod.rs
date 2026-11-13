//! ChatGPT plan usage, read from the logs the Codex CLI writes, with no network at all. A refresh runs on its own
//! thread at most every 3 seconds, or 2 seconds apart when asked by hand.

#[cfg(test)]
mod fixture;
mod home;
mod refresh;
mod rollouts;
mod scan;
mod slots;
mod snapshot;
mod state;
mod tail;
mod timestamp;

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::{PlanUsage, UsageWindow, Wake, lock};
use state::{Outcome, Shared};

const REFRESH_EVERY: Duration = Duration::from_secs(3);
const MANUAL_DEBOUNCE: Duration = Duration::from_secs(2);
const SPAWN_FAILED: &str = "Couldn't start the usage refresh";

pub struct Codex {
    shared: Arc<Mutex<Shared>>,
    wake: Wake,
    last_start: Option<Instant>,
    seen_version: u64,
}

impl Codex {
    pub fn new(wake: Wake) -> Self {
        Self { shared: Arc::default(), wake, last_start: None, seen_version: 0 }
    }

    pub fn poll(&mut self) {
        self.start_after(REFRESH_EVERY);
    }

    pub fn force(&mut self) {
        self.start_after(MANUAL_DEBOUNCE);
    }

    pub fn busy(&self) -> bool {
        lock(&self.shared).in_flight
    }

    pub fn latest(&mut self) -> Option<PlanUsage> {
        let shared = lock(&self.shared);
        (shared.version != self.seen_version).then(|| {
            self.seen_version = shared.version;
            shared.usage.clone()
        })
    }

    fn start_after(&mut self, gap: Duration) {
        let now = Instant::now();
        if self.last_start.is_none_or(|at| now.duration_since(at) >= gap) {
            self.start(now);
        }
    }

    fn start(&mut self, now: Instant) {
        if !lock(&self.shared).begin() {
            return;
        }
        self.last_start = Some(now);
        let shared = Arc::clone(&self.shared);
        let wake = Arc::clone(&self.wake);
        let spawned = thread::Builder::new().name("notchbase-codex".into()).spawn(move || {
            let in_flight = InFlight(shared);
            refresh::refresh(&in_flight.0);
            drop(in_flight);
            wake();
        });
        if spawned.is_err() {
            let mut shared = lock(&self.shared);
            shared.in_flight = false;
            shared.apply(Outcome::Failed(SPAWN_FAILED.to_owned()));
        }
    }
}

/// Clears the in-flight flag when the refresh ends, even if it panics.
struct InFlight(Arc<Mutex<Shared>>);

impl Drop for InFlight {
    fn drop(&mut self) {
        lock(&self.0).in_flight = false;
    }
}
