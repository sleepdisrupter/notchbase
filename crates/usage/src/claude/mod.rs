//! Claude plan usage. The Claude desktop app's history file is read first because it costs nothing, and the usage
//! endpoint is asked only when that is missing or more than 20 minutes old.

mod api;
mod credentials;
mod desktop;
mod json;
mod refresh;
mod response;
mod rfc3339;
mod state;

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use crate::{Wake, lock};
use state::{Outcome, Shared};

/// Usage older than this is stale. The app's history isn't used as is, and the card shows the age more plainly.
pub const USAGE_STALE_AFTER: Duration = Duration::from_secs(20 * 60);
const REFRESH_EVERY: Duration = Duration::from_secs(3);
const MANUAL_DEBOUNCE: Duration = Duration::from_secs(2);

/// One usage window: the share used, when it resets, its length in minutes, and whether any of that is guessed.
#[derive(Clone, Debug, PartialEq)]
pub struct UsageWindow {
    pub used: f32,
    pub resets_at: Option<SystemTime>,
    pub estimated: bool,
    pub minutes: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum PlanUsage {
    #[default]
    Loading,
    Ready {
        five_hour: Option<UsageWindow>,
        weekly: Option<UsageWindow>,
        fetched_at: SystemTime,
    },
    SignedOut,
    Expired,
    Error(String),
}

pub struct Claude {
    shared: Arc<Mutex<Shared>>,
    wake: Wake,
    last_start: Option<Instant>,
    seen_version: u64,
}

impl Claude {
    pub fn new(wake: Wake) -> Self {
        Self { shared: Arc::default(), wake, last_start: None, seen_version: 0 }
    }

    pub fn poll(&mut self) {
        self.start_after(REFRESH_EVERY, false);
    }

    pub fn force(&mut self) {
        self.start_after(MANUAL_DEBOUNCE, true);
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

    fn start_after(&mut self, gap: Duration, manual: bool) {
        let now = Instant::now();
        if self.last_start.is_none_or(|at| now.duration_since(at) >= gap) {
            self.start(now, manual);
        }
    }

    fn start(&mut self, now: Instant, manual: bool) {
        if !lock(&self.shared).begin(manual) {
            return;
        }
        self.last_start = Some(now);
        let shared = Arc::clone(&self.shared);
        let wake = Arc::clone(&self.wake);
        let spawned = thread::Builder::new().name("notchbase-claude".into()).spawn(move || {
            let in_flight = InFlight(shared);
            refresh::refresh(&in_flight.0, manual);
            drop(in_flight);
            wake();
        });
        if spawned.is_err() {
            let mut shared = lock(&self.shared);
            shared.in_flight = false;
            shared.apply(Outcome::failed("Couldn't start the usage request"), now);
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
