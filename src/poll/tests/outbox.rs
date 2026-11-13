use std::sync::Arc;

use crate::poll::Outbox;

#[test]
fn items_come_out_in_order() {
    let outbox = Outbox::default();
    outbox.push(1);
    outbox.extend([2, 3]);
    outbox.push(4);
    assert_eq!(outbox.take_all(), [1, 2, 3, 4]);
}

#[test]
fn taking_empties_the_outbox() {
    let outbox = Outbox::default();
    outbox.push("one");
    assert!(!outbox.is_empty());
    assert_eq!(outbox.take_all(), ["one"]);
    assert!(outbox.is_empty());
    assert!(outbox.take_all().is_empty());
}

#[test]
fn fresh_outbox_is_empty() {
    let outbox = Outbox::<u8>::default();
    assert!(outbox.is_empty());
    assert!(outbox.take_all().is_empty());
}

#[test]
fn extending_with_nothing_adds_nothing() {
    let outbox = Outbox::<u8>::default();
    outbox.extend(Vec::new());
    assert!(outbox.is_empty());
}

#[test]
fn pushes_from_other_threads_arrive() {
    let outbox = Arc::new(Outbox::default());
    let senders: Vec<_> = (0..4)
        .map(|thread| {
            let outbox = Arc::clone(&outbox);
            std::thread::spawn(move || (0..25).for_each(|n| outbox.push(thread * 100 + n)))
        })
        .collect();
    senders.into_iter().for_each(|sender| sender.join().unwrap());
    let mut arrived = outbox.take_all();
    arrived.sort_unstable();
    let expected: Vec<i32> = (0..4).flat_map(|thread| (0..25).map(move |n| thread * 100 + n)).collect();
    assert_eq!(arrived, expected);
}
