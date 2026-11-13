use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::poll::{Mailbox, Wake};

fn counted() -> (Wake, Arc<AtomicUsize>) {
    let woken = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&woken);
    let wake: Wake = Arc::new(move || {
        counter.fetch_add(1, Ordering::SeqCst);
    });
    (wake, woken)
}

#[test]
fn every_post_wakes_the_reader() {
    let (wake, woken) = counted();
    let mailbox = Mailbox::new(wake);
    mailbox.post("first");
    mailbox.post("second");
    assert_eq!(woken.load(Ordering::SeqCst), 2);
    assert_eq!(mailbox.take(), ["first", "second"]);
}

#[test]
fn taking_drains_without_waking() {
    let (wake, woken) = counted();
    let mailbox = Mailbox::new(wake);
    mailbox.post(1);
    assert_eq!(mailbox.take(), [1]);
    assert!(mailbox.take().is_empty());
    assert_eq!(woken.load(Ordering::SeqCst), 1);
}

#[test]
fn empty_mailbox_has_nothing_to_take() {
    let (wake, woken) = counted();
    assert!(Mailbox::<u8>::new(wake).take().is_empty());
    assert_eq!(woken.load(Ordering::SeqCst), 0);
}

#[test]
fn clones_share_one_mailbox() {
    let (wake, woken) = counted();
    let reader = Mailbox::new(wake);
    let writer = reader.clone();
    let sent = std::thread::spawn(move || writer.post(String::from("from a thread")));
    sent.join().unwrap();
    assert_eq!(reader.take(), ["from a thread"]);
    assert_eq!(woken.load(Ordering::SeqCst), 1);
}
