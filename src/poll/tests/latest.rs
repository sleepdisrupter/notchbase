use crate::poll::Latest;

#[test]
fn reading_is_handed_out_once() {
    let latest = Latest::default();
    assert!(latest.publish(1));
    assert_eq!(latest.take(), Some(1));
    assert_eq!(latest.take(), None);
}

#[test]
fn nothing_before_the_first_reading() {
    assert_eq!(Latest::<u8>::default().take(), None);
}

#[test]
fn same_reading_isnt_published_twice() {
    let latest = Latest::default();
    latest.publish("calm");
    latest.take();
    assert!(!latest.publish("calm"));
    assert_eq!(latest.take(), None);
}

#[test]
fn repeat_keeps_the_untaken_reading() {
    let latest = Latest::default();
    latest.publish(4);
    assert!(!latest.publish(4));
    assert_eq!(latest.take(), Some(4));
}

#[test]
fn newer_reading_replaces_untaken_one() {
    let latest = Latest::default();
    latest.publish(1);
    assert!(latest.publish(2));
    assert_eq!(latest.take(), Some(2));
    assert_eq!(latest.take(), None);
}

#[test]
fn going_back_to_earlier_is_news() {
    let latest = Latest::default();
    latest.publish(1);
    latest.publish(2);
    assert!(latest.publish(1));
    assert_eq!(latest.take(), Some(1));
}

#[test]
fn readings_are_compared_by_value() {
    let latest = Latest::default();
    latest.publish(String::from("\u{1f3a7} headphones"));
    latest.take();
    assert!(!latest.publish(String::from("\u{1f3a7} headphones")));
    assert!(latest.publish(String::from("\u{1f3a7} speakers")));
}
