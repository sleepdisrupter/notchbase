//! Waiting on AVFoundation's callbacks while watching for a cancel and for a callback that never comes.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use crate::shelf::Task;

const REPLY_TIMEOUT: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(100);

pub(super) enum Wait<T> {
    Ready(T),
    Cancelled,
    Abandoned,
    Stalled,
}

pub(super) fn reply<T>(start: impl FnOnce(Sender<T>)) -> Option<T> {
    let (sender, receiver) = mpsc::channel();
    start(sender);
    receiver.recv_timeout(REPLY_TIMEOUT).ok()
}

pub(super) fn until<T>(
    receiver: &Receiver<T>,
    task: &mut Task,
    patience: Option<Duration>,
    mut tick: impl FnMut(&mut Task),
) -> Wait<T> {
    let deadline = patience.and_then(|patience| Instant::now().checked_add(patience));
    loop {
        if task.cancelled() {
            return Wait::Cancelled;
        }
        match receiver.recv_timeout(POLL_INTERVAL) {
            Ok(value) => return Wait::Ready(value),
            Err(RecvTimeoutError::Disconnected) => return Wait::Abandoned,
            Err(RecvTimeoutError::Timeout) if deadline.is_some_and(|deadline| Instant::now() >= deadline) => {
                return Wait::Stalled;
            }
            Err(RecvTimeoutError::Timeout) => tick(task),
        }
    }
}
