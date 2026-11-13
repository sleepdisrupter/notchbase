//! While the mixer is in use, meters update every 33 ms and the app and device lists once a second.

use std::time::{Duration, Instant};

use crate::audio::imp;

pub const TICK: Duration = Duration::from_millis(33);
const REFRESH: Duration = Duration::from_secs(1);

pub struct Cadence {
    last_refresh: Option<Instant>,
    next_tick: Instant,
}

impl Cadence {
    pub fn new(now: Instant) -> Self {
        Self { last_refresh: None, next_tick: now }
    }

    pub fn is_due(&self, now: Instant) -> bool {
        now >= self.next_tick
    }

    #[cfg(target_os = "windows")]
    pub fn has_refreshed(&self) -> bool {
        self.last_refresh.is_some()
    }

    #[cfg(target_os = "windows")]
    pub fn next_tick(&self) -> Instant {
        self.next_tick
    }

    pub fn step(&mut self, engine: &mut imp::Backend, now: Instant) {
        if self.last_refresh.is_none_or(|t| now.duration_since(t) >= REFRESH) {
            engine.refresh();
            self.last_refresh = Some(now);
        }
        engine.update_levels();
        self.next_tick = now + TICK;
    }
}
