use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use super::PATIENCE;
use crate::poll::Poll;

fn wait_for<T: Clone + PartialEq + Send + 'static>(poll: &Poll<T>) -> T {
    let deadline = Instant::now() + PATIENCE;
    loop {
        if let Some(reading) = poll.take() {
            return reading;
        }
        assert!(Instant::now() < deadline, "the poll never published a reading");
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn published_once_until_it_changes() {
    let step = Duration::from_millis(5);
    let poll = Poll::start("notchbase-test-poll", step, || {
        let mut reads = 0u32;
        move || {
            reads += 1;
            reads.min(40) / 40
        }
    });
    assert_eq!(wait_for(&poll), 0);
    assert_eq!(poll.take(), None, "the same reading is not handed out twice");
    assert_eq!(wait_for(&poll), 1);
}

#[test]
fn reader_opens_on_the_polling_thread() {
    let poll = Poll::start("notchbase-test-open", Duration::from_millis(5), || {
        let thread = std::thread::current().name().map(str::to_owned);
        move || thread.clone()
    });
    assert_eq!(wait_for(&poll).as_deref(), Some("notchbase-test-open"));
}

#[test]
fn dropping_a_poll_stops_its_readings() {
    let reads = Arc::new(AtomicU32::new(0));
    let counter = Arc::clone(&reads);
    let poll = Poll::start("notchbase-test-stop", Duration::from_millis(1), move || {
        move || counter.fetch_add(1, Ordering::SeqCst)
    });
    super::eventually("a few readings", || reads.load(Ordering::SeqCst) >= 3);
    drop(poll);
    std::thread::sleep(Duration::from_millis(5));
    let after_drop = reads.load(Ordering::SeqCst);
    std::thread::sleep(Duration::from_millis(5));
    assert!(reads.load(Ordering::SeqCst) <= after_drop + 1, "at most the reading under way finishes");
}
