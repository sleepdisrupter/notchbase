//! The Windows driver. The backend runs on its own thread and sleeps until a command or a page wakes it. If the thread
//! dies, the mixer says it stopped instead of taking the app down.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvError, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use super::cadence::Cadence;
use super::publisher::Publisher;
use super::{Command, DEMAND_GRACE, Snapshot};
use crate::audio::imp;

const THREAD_NAME: &str = "notchbase-audio";
const WORKER_UNAVAILABLE: &str = "Volume control couldn't start";
const WORKER_STOPPED: &str = "Volume control stopped unexpectedly";

enum Message {
    Command(Command),
    Wake,
}

struct Shared {
    latest: Mutex<Option<Snapshot>>,
    demand: AtomicU64,
    epoch: Instant,
}

impl Shared {
    fn new() -> Self {
        Self { latest: Mutex::new(None), demand: AtomicU64::new(0), epoch: Instant::now() }
    }

    fn stamp(&self) -> u64 {
        self.epoch.elapsed().as_millis() as u64 + 1
    }

    fn active(&self) -> bool {
        let last = self.demand.load(Ordering::Relaxed);
        last != 0 && self.stamp().saturating_sub(last) < DEMAND_GRACE.as_millis() as u64
    }

    fn publish(&self, snapshot: Snapshot) {
        *self.latest.lock().unwrap_or_else(PoisonError::into_inner) = Some(snapshot);
    }

    fn take(&self) -> Option<Snapshot> {
        self.latest.lock().unwrap_or_else(PoisonError::into_inner).take()
    }
}

struct PanicGuard<'a>(&'a Shared);

impl Drop for PanicGuard<'_> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            self.0.publish(unsupported(WORKER_STOPPED));
        }
    }
}

pub struct Worker {
    messages: Sender<Message>,
    shared: Arc<Shared>,
}

impl Worker {
    pub fn start() -> (Self, Snapshot) {
        let (messages, inbox) = mpsc::channel();
        let shared = Arc::new(Shared::new());
        let worker_shared = Arc::clone(&shared);
        let spawned = std::thread::Builder::new().name(THREAD_NAME.into()).spawn(move || run(inbox, &worker_shared));
        let initial = match spawned {
            Ok(_) => Snapshot { supported: true, ..Snapshot::default() },
            Err(_) => unsupported(WORKER_UNAVAILABLE),
        };
        (Self { messages, shared }, initial)
    }

    pub fn demand(&mut self) {
        let was_active = self.shared.active();
        self.shared.demand.store(self.shared.stamp(), Ordering::Relaxed);
        if !was_active {
            let _ = self.messages.send(Message::Wake);
        }
    }

    pub fn take(&mut self) -> Option<Snapshot> {
        self.shared.take()
    }

    pub fn send(&mut self, command: Command) {
        let _ = self.messages.send(Message::Command(command));
    }

    pub fn maintain(&mut self) {}
}

// Numbered after every command, so the mixer always takes it.
fn unsupported(reason: &str) -> Snapshot {
    Snapshot { supported: false, reason: Some(reason.to_owned()), seq: u64::MAX, ..Snapshot::default() }
}

fn run(inbox: Receiver<Message>, shared: &Shared) {
    let _guard = PanicGuard(shared);
    let mut engine = imp::Backend::new();
    let mut publisher = Publisher::default();
    let mut cadence = Cadence::new(Instant::now());
    let mut seq = 0u64;
    while let Ok(first) = next_message(&inbox, shared, &cadence) {
        let mut changed = false;
        for message in first.into_iter().chain(inbox.try_iter()) {
            if let Message::Command(command) = message {
                command.apply(&mut engine);
                seq += 1;
                changed = true;
            }
        }
        let now = Instant::now();
        let first_pass = !cadence.has_refreshed();
        if (first_pass || shared.active()) && cadence.is_due(now) {
            cadence.step(&mut engine, now);
            changed = true;
        }
        if changed && let Some(snapshot) = publisher.capture(&engine, seq, now) {
            shared.publish(snapshot);
        }
    }
}

fn next_message(inbox: &Receiver<Message>, shared: &Shared, cadence: &Cadence) -> Result<Option<Message>, RecvError> {
    if !cadence.has_refreshed() {
        return Ok(None);
    }
    if !shared.active() {
        return inbox.recv().map(Some);
    }
    match inbox.recv_timeout(cadence.next_tick().saturating_duration_since(Instant::now())) {
        Ok(message) => Ok(Some(message)),
        Err(RecvTimeoutError::Timeout) => Ok(None),
        Err(RecvTimeoutError::Disconnected) => Err(RecvError),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn worker_publishes_snapshots() {
        let (mut driver, initial) = Worker::start();
        assert!(initial.supported);
        driver.demand();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut got = None;
        while Instant::now() < deadline && got.is_none() {
            got = driver.take();
            std::thread::sleep(Duration::from_millis(10));
        }
        let snapshot = got.expect("worker published a snapshot");
        assert!(snapshot.supported);
    }

    #[test]
    fn dead_worker_reports_unsupported() {
        let shared = Arc::new(Shared::new());
        let watched = Arc::clone(&shared);
        let died = std::thread::spawn(move || {
            let _guard = PanicGuard(&watched);
            std::panic::resume_unwind(Box::new(()));
        })
        .join();
        assert!(died.is_err());
        let last = shared.take().expect("the dying worker published its state");
        assert!(!last.supported);
        assert_eq!(last.reason.as_deref(), Some(WORKER_STOPPED));
        assert_eq!(last.seq, u64::MAX, "outranks any command the UI already sent");
    }
}
