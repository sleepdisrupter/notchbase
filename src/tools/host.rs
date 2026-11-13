//! Runs one tool: queues its events, fires its timers, and catches its panics, so a failing tool stops alone.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use notchbase_sdk::Driver;
use notchbase_sdk::ffi::panic_message;
use notchbase_sdk::protocol::{Effects, Event, Manifest, Request, Start, Views, Wants};

use super::ToolSlot;

pub struct ToolHost {
    pub slot: ToolSlot,
    pub manifest: Manifest,
    driver: Box<dyn Driver>,
    woken: Arc<AtomicBool>,
    wants: Wants,
    timers: Vec<(u64, Instant)>,
    inbox: Vec<Event>,
    revision: u64,
    seen: Option<u64>,
    views: Views,
    failure: Option<String>,
}

impl ToolHost {
    pub fn new(slot: ToolSlot, driver: Box<dyn Driver>, manifest: Manifest, woken: Arc<AtomicBool>) -> Self {
        Self {
            slot,
            manifest,
            driver,
            woken,
            wants: Wants::default(),
            timers: Vec::new(),
            inbox: Vec::new(),
            revision: 0,
            seen: None,
            views: Views::default(),
            failure: None,
        }
    }

    pub fn id(&self) -> &str {
        &self.manifest.id
    }

    pub fn revision(&self) -> (u64, bool) {
        (self.revision, self.failure.is_some())
    }

    pub fn wants(&self) -> &Wants {
        &self.wants
    }

    pub fn failure(&self) -> Option<&str> {
        self.failure.as_deref()
    }

    pub fn start(&mut self, start: Start, now: Instant) -> Vec<Request> {
        let effects = self.guard(|driver| driver.start(start));
        self.absorb(effects, now)
    }

    pub fn queue(&mut self, event: Event) {
        if self.failure.is_none() {
            self.inbox.push(event);
        }
    }

    pub fn pump(&mut self, now: Instant) -> Vec<Request> {
        if self.failure.is_some() {
            return Vec::new();
        }
        for (every, next) in &mut self.timers {
            if now >= *next {
                self.inbox.push(Event::Tick { every_ms: *every });
                *next = advance(*next, *every, now);
            }
        }
        let woken = self.woken.swap(false, Ordering::AcqRel);
        if self.inbox.is_empty() && !woken {
            return Vec::new();
        }
        let events = std::mem::take(&mut self.inbox);
        let effects = self.guard(|driver| driver.handle(events));
        self.absorb(effects, now)
    }

    /// Asks the tool for its views only when its revision moved.
    pub fn views(&mut self) -> &Views {
        if self.failure.is_none() && self.seen != Some(self.revision) {
            self.seen = Some(self.revision);
            if let Some(views) = self.guard(|driver| driver.views()) {
                self.views = views;
            }
        }
        &self.views
    }

    pub fn next_wakeup(&self) -> Option<Instant> {
        self.timers.iter().map(|(_, next)| *next).min()
    }

    fn absorb(&mut self, effects: Option<Effects>, now: Instant) -> Vec<Request> {
        let Some(effects) = effects else { return Vec::new() };
        self.revision = effects.revision;
        if effects.wants.every_ms != self.wants.every_ms {
            self.timers =
                effects.wants.every_ms.iter().map(|&every| (every, now + Duration::from_millis(every))).collect();
        }
        self.wants = effects.wants;
        effects.requests
    }

    /// Calls into the tool. A panic stops this tool for good, clearing its inbox, timers and views.
    fn guard<T>(&mut self, call: impl FnOnce(&mut dyn Driver) -> T) -> Option<T> {
        match catch_unwind(AssertUnwindSafe(|| call(self.driver.as_mut()))) {
            Ok(value) => Some(value),
            Err(panic) => {
                let message = panic_message(panic.as_ref());
                eprintln!("notchbase: the {} tool stopped: {message}", self.manifest.id);
                self.failure = Some(message);
                self.inbox.clear();
                self.timers.clear();
                self.views = Views::default();
                None
            }
        }
    }
}

/// The next tick after `now`, so a late timer skips the missed ticks instead of firing them all at once.
fn advance(next: Instant, every_ms: u64, now: Instant) -> Instant {
    let every = Duration::from_millis(every_ms.max(1));
    let behind = now.saturating_duration_since(next).as_nanos() / every.as_nanos().max(1);
    next + every * (behind as u32 + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn late_timer_skips_ahead() {
        let t0 = Instant::now();
        assert_eq!(advance(t0, 100, t0), t0 + Duration::from_millis(100));
        assert_eq!(advance(t0, 100, t0 + Duration::from_millis(350)), t0 + Duration::from_millis(400));
    }
}
