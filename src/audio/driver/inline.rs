//! The driver for macOS and other systems: the backend runs on the main thread, called from the frame loop.

use std::time::Instant;

use super::cadence::Cadence;
use super::publisher::Publisher;
use super::{Command, DEMAND_GRACE, Snapshot};
use crate::audio::imp;

pub struct Inline {
    engine: imp::Backend,
    publisher: Publisher,
    cadence: Cadence,
    seq: u64,
    demanded: Option<Instant>,
    dirty: bool,
}

impl Inline {
    pub fn start() -> (Self, Snapshot) {
        let engine = imp::Backend::new();
        let mut publisher = Publisher::default();
        let now = Instant::now();
        let initial = publisher.capture(&engine, 0, now).unwrap_or_default();
        let driver = Self { engine, publisher, cadence: Cadence::new(now), seq: 0, demanded: None, dirty: false };
        (driver, initial)
    }

    pub fn demand(&mut self) {
        self.demanded = Some(Instant::now());
    }

    pub fn take(&mut self) -> Option<Snapshot> {
        let now = Instant::now();
        let active = self.demanded.is_some_and(|t| now.duration_since(t) < DEMAND_GRACE);
        let tick = active && self.cadence.is_due(now);
        if !tick && !self.dirty {
            return None;
        }
        if tick {
            self.cadence.step(&mut self.engine, now);
        }
        self.dirty = false;
        self.publisher.capture(&self.engine, self.seq, now)
    }

    pub fn send(&mut self, command: Command) {
        command.apply(&mut self.engine);
        self.seq += 1;
        self.dirty = true;
    }

    pub fn maintain(&mut self) {
        self.engine.maintain();
    }
}
