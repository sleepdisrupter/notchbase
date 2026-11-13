use std::sync::atomic::Ordering;

use super::*;
use crate::find::context::{DAY, MINUTE};

const SECOND: Duration = Duration::from_secs(1);

fn settled(name: &str) -> Rig {
    let mut rig = Rig::new(name);
    rig.writer.launch();
    let jobs = rig.scanner.take();
    let root = rig.root();
    rig.listed(vec![listing(&root, &[file("a.txt", 5), file("b.txt", 5)], jobs[0].ticket)]);
    rig.finish(&jobs);
    rig.world.pass(Duration::from_secs(61));
    rig.writer.tick();
    rig
}

fn waits(rig: &Rig) -> Duration {
    rig.writer.wait()
}

#[test]
fn idle_writer_sleeps_to_deadline() {
    let rig = settled("idle-sleeps");
    let reconcile = Duration::from_secs((7 * DAY).into());
    assert!(waits(&rig) > reconcile - Duration::from_secs(120), "nothing is due before the weekly reconcile");
    assert!(waits(&rig) <= reconcile);
}

#[test]
fn change_waits_for_hourly_save() {
    let mut rig = settled("idle-save");
    rig.writer.obey(Order::Gone(rig.at("a.txt")));
    let hour = Duration::from_secs((60 * MINUTE).into());
    assert!(waits(&rig) > hour - Duration::from_secs(120) && waits(&rig) <= hour);
    rig.world.pass(hour);
    assert!(waits(&rig).is_zero(), "the save is due");
    rig.writer.tick();
    assert!(waits(&rig) > hour, "saved, nothing is due for a long time");
}

#[test]
fn busy_writer_ticks_every_second() {
    let mut rig = settled("idle-visible");
    rig.writer.shared.visible.store(true, Ordering::Release);
    assert!(waits(&rig) <= SECOND);
    rig.writer.shared.visible.store(false, Ordering::Release);
    rig.writer.obey(Order::Changes(Burst { events: Vec::new(), caught_up: true }));
    rig.events(vec![(Change::Relist(rig.root()), 9)]);
    assert!(waits(&rig) < SECOND, "a change waiting to settle is looked at soon");
}

#[test]
fn disabled_writer_sleeps() {
    let mut rig = settled("idle-off");
    rig.writer.obey(Order::Follow(FindConfig { enabled: false, ..FindConfig::default() }));
    assert_eq!(waits(&rig), Duration::MAX);
}
