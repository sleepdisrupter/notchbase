mod latest;
mod mailbox;
mod outbox;
mod polling;
mod worker;

use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::lock;

const PATIENCE: Duration = Duration::from_secs(5);
const STEP: Duration = Duration::from_millis(2);

fn eventually(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + PATIENCE;
    while !done() {
        assert!(Instant::now() < deadline, "{what} never happened");
        std::thread::sleep(STEP);
    }
}

#[test]
fn lock_hands_over_what_it_guards() {
    let guarded = Mutex::new(vec![1, 2]);
    lock(&guarded).push(3);
    assert_eq!(*lock(&guarded), [1, 2, 3]);
}

#[test]
fn poisoned_lock_is_still_handed_over() {
    let guarded = std::sync::Arc::new(Mutex::new(7));
    let holder = std::sync::Arc::clone(&guarded);
    let panicked = std::thread::spawn(move || {
        let _held = holder.lock().unwrap();
        panic!("the holder gives up");
    })
    .join();
    assert!(panicked.is_err() && guarded.is_poisoned());
    *lock(&guarded) += 1;
    assert_eq!(*lock(&guarded), 8);
}
