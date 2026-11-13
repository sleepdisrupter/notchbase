//! A named thread and the shift it works. The shift holds the state both sides share, says when to stop, and naps
//! in short steps so that a stop is noticed quickly.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub struct Shift<S> {
    stop: AtomicBool,
    nap: Duration,
    state: S,
}

impl<S> Shift<S> {
    /// True until the worker is dropped.
    pub fn on_duty(&self) -> bool {
        !self.stop.load(Ordering::Relaxed)
    }

    pub fn state(&self) -> &S {
        &self.state
    }

    /// Sleeps until `until`, waking early once `interrupted` says so or the worker is dropped.
    pub fn nap_until(&self, until: Instant, interrupted: impl Fn(&S) -> bool) {
        while self.on_duty() && !interrupted(&self.state) {
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return;
            }
            std::thread::sleep(self.nap.min(left));
        }
    }
}

/// Owns the thread's shift. Dropping it tells the thread to stop, without waiting for it.
pub struct Worker<S> {
    shift: Arc<Shift<S>>,
}

impl<S: Send + Sync + 'static> Worker<S> {
    pub fn start(thread: &str, nap: Duration, state: S, work: impl FnOnce(&Shift<S>) + Send + 'static) -> Self {
        let shift = Arc::new(Shift { stop: AtomicBool::new(false), nap, state });
        let working = Arc::clone(&shift);
        let _ = std::thread::Builder::new().name(thread.into()).spawn(move || work(&working));
        Self { shift }
    }
}

impl<S> Worker<S> {
    pub fn state(&self) -> &S {
        &self.shift.state
    }
}

impl<S> Drop for Worker<S> {
    fn drop(&mut self) {
        self.shift.stop.store(true, Ordering::Relaxed);
    }
}
