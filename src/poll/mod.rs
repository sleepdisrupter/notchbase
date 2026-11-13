//! The pieces background threads share: workers that stop when dropped, polls that publish a reading only when it
//! changes, and outboxes and mailboxes that carry items back to the app.

mod latest;
mod mailbox;
mod outbox;
mod worker;

#[cfg(test)]
mod tests;

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

pub use latest::Latest;
pub use mailbox::Mailbox;
pub use outbox::Outbox;
pub use worker::{Shift, Worker};

/// Wakes the app's event loop, so it looks at what changed.
pub type Wake = Arc<dyn Fn() + Send + Sync>;

/// A poll sleeps in steps this long, so dropping it stops the thread within one.
const NAP: Duration = Duration::from_millis(100);

/// Locks a mutex, and still hands it over if a thread panicked holding it.
pub fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A named thread that reads a value on an interval and publishes it only when it differs from the last one.
pub struct Poll<T> {
    worker: Worker<Latest<T>>,
}

impl<T: Clone + PartialEq + Send + 'static> Poll<T> {
    /// Starts the thread. `open` runs on it, so the reader it makes may hold state that isn't `Send`.
    pub fn start<Open, Read>(thread: &str, every: Duration, open: Open) -> Self
    where
        Open: FnOnce() -> Read + Send + 'static,
        Read: FnMut() -> T,
    {
        let worker = Worker::start(thread, NAP, Latest::default(), move |shift| {
            let mut read = open();
            while shift.on_duty() {
                shift.state().publish(read());
                shift.nap_until(Instant::now() + every, |_| false);
            }
        });
        Self { worker }
    }

    /// The newest reading the app hasn't taken yet.
    pub fn take(&self) -> Option<T> {
        self.worker.state().take()
    }
}
