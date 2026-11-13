use std::sync::atomic::Ordering;

use super::*;
use crate::find::context::{DAY, MINUTE};
use crate::find::model::MERGE_ROWS;
use crate::find::writer::upkeep::cap;

const GIB: u64 = 1 << 30;
const FIRST_SCAN: usize = 1_450_000;
const BATCH: usize = 4_096;

fn quiet(rig: &mut Rig, time: Duration) {
    rig.world.pass(time);
    rig.writer.tick();
}

fn complete_root(name: &str, scanned_ago: Stamp, swept_ago: Stamp, watches: bool) -> Rig {
    let mut rig = Rig::new(name);
    rig.world.state().watches = watches;
    rig.writer.launch();
    let jobs = rig.scanner.take();
    rig.finish(&jobs);
    let root = rig.root();
    let found = rig.writer.mark_of(&root).unwrap();
    (found.scanned_at, found.swept_at) = (START - scanned_ago, START - swept_ago);
    rig
}

fn reaches(rig: &Rig) -> Vec<u8> {
    rig.scanner.take().iter().map(|job| job.reach).collect()
}

fn files(count: usize, from: usize) -> Vec<String> {
    (from..from + count).map(|n| format!("{n}.txt")).collect()
}

#[test]
fn budget_fits_the_rebuild_peak() {
    assert_eq!(cap(8 * GIB, 0), 2_936_012);
    assert_eq!(cap(16 * GIB, 0), 6_770_805);
    assert_eq!(cap(64 * GIB, 0), cap(16 * GIB, 0), "the peak stops growing at 512 MiB");
    assert_eq!(cap(2 * GIB, 0), 1_977_314, "and never drops below 192 MiB");
    assert_eq!((cap(0, 1_000), cap(0, u64::MAX)), (1_000, 50_000_000), "a number in the config wins, up to 50M");
}

#[test]
fn escalation_stops_at_five() {
    let config = FindConfig { max_entries: 4, ..FindConfig::default() };
    let mut rig = Rig::with("valve", config, Scratch::new("valve-store"));
    rig.writer.launch();
    let names = files(10, 0);
    let entries: Vec<(&str, u8, u32)> = names.iter().map(|name| file(name, 1)).collect();
    rig.listed(vec![listing(&rig.root(), &entries, 1)]);
    assert_eq!(*lock(&rig.scanner.levels), [0, 1, 2, 3, 4, 5]);
    assert_eq!(rig.writer.meta.level, 5);
    assert_eq!(rig.writer.shared.reading().alive(), 11, "user content survives every level");
    assert!(lock(&rig.writer.shared.status).truncated);
    let mut more = listing(&rig.root(), &[file("more.txt", 1)], 1);
    more.complete = false;
    rig.listed(vec![more]);
    assert!(rig.find("more.txt").is_none(), "past the last level nothing is added");
}

#[test]
fn rebuilds_at_overlay_threshold() {
    let mut rig = Rig::new("threshold");
    rig.writer.launch();
    let (root, names) = (rig.root(), files(MERGE_ROWS, 0));
    let mut batches = names.chunks(BATCH).peekable();
    while let Some(chunk) = batches.next() {
        let entries: Vec<(&str, u8, u32)> = chunk.iter().map(|name| file(name, 1)).collect();
        let mut part = listing(&root, &entries, 1);
        part.complete = false;
        rig.listed(vec![part]);
        let generation = rig.writer.shared.reading().generation();
        assert_eq!(generation, u64::from(batches.peek().is_none()), "one rebuild, when the threshold is crossed");
    }
    let live = rig.writer.shared.reading();
    assert!(live.settled() && live.alive() == MERGE_ROWS + 1);
}

#[test]
#[ignore = "lists 1.45M names through the writer; run with --release"]
fn big_first_scan_rebuilds_six_times() {
    let mut rig = Rig::new("first-scan");
    rig.writer.launch();
    let jobs = rig.scanner.take();
    let folders: Vec<String> = (0..FIRST_SCAN.div_ceil(BATCH)).map(|n| format!("folder {n}")).collect();
    let entries: Vec<(&str, u8, u32)> = folders.iter().map(|name| (name.as_str(), raw::DIR, 0)).collect();
    rig.listed(vec![listing(&rig.root(), &entries, 1)]);
    for (index, name) in folders.iter().enumerate() {
        let names = files(BATCH, index * BATCH);
        let entries: Vec<(&str, u8, u32)> = names.iter().map(|name| file(name, 1)).collect();
        rig.listed(vec![listing(&rig.root().join(name), &entries, 1)]);
    }
    rig.finish(&jobs);
    rig.writer.tick();
    let live = rig.writer.shared.reading();
    assert!(live.alive() > FIRST_SCAN);
    assert_eq!(live.generation(), 6, "at 65k, 131k, 262k, 524k and 1.05M rows, and once more to save");
}

#[test]
fn week_old_root_reconciled() {
    let mut rig = complete_root("week-old", 8 * DAY, DAY, true);
    rig.writer.tick();
    assert!(rig.scanner.take().is_empty(), "not quiet yet");
    rig.world.state().power = Power { level: Some(70), plugged: false, charging: false };
    quiet(&mut rig, Duration::from_secs(61));
    assert!(rig.scanner.take().is_empty(), "not on battery");
    rig.world.state().power.plugged = true;
    rig.writer.shared.visible.store(true, Ordering::Release);
    quiet(&mut rig, Duration::from_secs(31));
    assert!(rig.scanner.take().is_empty(), "not while someone is searching");
    rig.writer.shared.visible.store(false, Ordering::Release);
    rig.writer.tick();
    assert_eq!(reaches(&rig), [1]);
    assert_eq!(lock(&rig.scanner.paces).last(), Some(&Pace::Gentle));
}

#[test]
fn month_old_sweep_goes_deep() {
    let mut rig = complete_root("month-old", DAY, 31 * DAY, true);
    quiet(&mut rig, Duration::from_secs(61));
    assert_eq!(reaches(&rig), [3]);
}

#[test]
fn unwatched_root_every_half_hour() {
    let mut rig = complete_root("unwatched", 0, 0, false);
    quiet(&mut rig, Duration::from_secs(61));
    assert!(rig.scanner.take().is_empty());
    quiet(&mut rig, Duration::from_secs(30 * 60));
    let first = rig.scanner.take();
    assert_eq!(first.iter().map(|job| job.reach).collect::<Vec<_>>(), [1]);
    rig.finish(&first);
    assert_eq!(rig.writer.mark_of(&rig.root()).unwrap().scanned_at, START + 31 * MINUTE + 1);
    quiet(&mut rig, Duration::from_secs(29 * 60));
    assert!(rig.scanner.take().is_empty());
    quiet(&mut rig, Duration::from_secs(60));
    assert_eq!(reaches(&rig), [1]);
}

#[test]
fn unwatched_on_battery_is_gentle() {
    let mut rig = complete_root("unwatched-battery", 0, 0, false);
    rig.writer.tick();
    assert_eq!(lock(&rig.writer.shared.status).unwatched, 1);
    rig.world.state().power = Power { level: Some(70), plugged: false, charging: false };
    quiet(&mut rig, Duration::from_secs(31 * 60));
    assert_eq!(reaches(&rig), [1], "a root nobody watches still catches up on battery");
    assert_eq!(lock(&rig.scanner.paces).last(), Some(&Pace::Gentle));
}

#[test]
fn failed_watch_unwatches_root() {
    let mut rig = complete_root("watch-fails", 0, 0, true);
    quiet(&mut rig, Duration::from_secs(31 * 60));
    assert!(rig.scanner.take().is_empty(), "watched roots wait a week");
    rig.writer.obey(Order::Unwatched(rig.root()));
    quiet(&mut rig, Duration::from_secs(61));
    assert_eq!(reaches(&rig), [1]);
}

#[test]
fn dirty_index_saved_hourly() {
    let mut rig = complete_root("hourly", 0, 0, true);
    rig.writer.tick();
    let index = rig.store.file("index.bin");
    std::fs::remove_file(&index).unwrap();
    rig.listed(vec![listing(&rig.root(), &[file("a.txt", 1)], 1)]);
    rig.world.state().power = Power { level: Some(70), plugged: false, charging: false };
    quiet(&mut rig, Duration::from_secs(61 * 60));
    assert!(!index.exists());
    quiet(&mut rig, Duration::from_secs(5 * 60 * 60));
    assert!(index.exists());
}

#[test]
fn refresh_at_most_twice_a_second() {
    let mut rig = complete_root("refresh", 0, 0, true);
    rig.writer.shared.visible.store(true, Ordering::Release);
    rig.listed(vec![listing(&rig.root(), &[file("a.txt", 1)], 1)]);
    rig.listed(vec![listing(&rig.root(), &[file("a.txt", 1), file("b.txt", 1)], 1)]);
    let refreshes = |rig: &Rig| rig.asks.try_iter().filter(|asked| matches!(asked, Asked::Refresh)).count();
    assert_eq!(refreshes(&rig), 1);
    rig.world.pass(Duration::from_millis(500));
    rig.writer.tick();
    rig.writer.tick();
    assert_eq!(refreshes(&rig), 1, "the change that waited, then nothing more");
}

#[test]
fn progress_four_times_a_second() {
    let mut rig = complete_root("progress", 0, 0, true);
    let version = |rig: &Rig| rig.writer.shared.version.load(Ordering::Acquire);
    let hidden = version(&rig);
    rig.listed(vec![listing(&rig.root(), &[file("a.txt", 1)], 1)]);
    assert_eq!(version(&rig), hidden, "hidden: counted, not announced");
    rig.writer.shared.visible.store(true, Ordering::Release);
    rig.world.pass(Duration::from_millis(250));
    rig.listed(vec![listing(&rig.root(), &[file("a.txt", 1), file("b.txt", 1)], 1)]);
    rig.listed(vec![listing(&rig.root(), &[file("a.txt", 1), file("b.txt", 1), file("c.txt", 1)], 1)]);
    assert_eq!(version(&rig), hidden + 1);
    rig.world.pass(Duration::from_millis(250));
    rig.writer.tick();
    assert_eq!(version(&rig), hidden + 2, "the count that waited");
    assert_eq!(lock(&rig.writer.shared.status).entries, 4);
}

#[test]
fn low_battery_pauses_scan() {
    let mut rig = complete_root("battery", 0, 0, true);
    rig.world.state().power = Power { level: Some(10), plugged: false, charging: false };
    quiet(&mut rig, Duration::from_secs(31));
    assert_eq!(lock(&rig.scanner.paces).last(), Some(&Pace::Paused));
    rig.world.state().power = Power { level: Some(60), plugged: false, charging: false };
    rig.writer.shared.visible.store(true, Ordering::Release);
    quiet(&mut rig, Duration::from_secs(31));
    assert_eq!(lock(&rig.scanner.paces).last(), Some(&Pace::Full), "someone is waiting for results");
}

#[test]
fn probes_every_thirty_seconds() {
    let mut rig = Rig::new("probe");
    rig.world.state().full = false;
    rig.writer.launch();
    rig.scanner.take();
    rig.writer.shared.visible.store(true, Ordering::Release);
    rig.world.state().full = true;
    quiet(&mut rig, Duration::from_secs(29));
    assert_eq!(lock(&rig.writer.shared.status).access, crate::find::view::Access::Limited);
    quiet(&mut rig, Duration::from_secs(1));
    assert_eq!(lock(&rig.writer.shared.status).access, crate::find::view::Access::Full);
}

#[test]
fn hidden_find_never_probes() {
    let mut rig = Rig::new("probe-hidden");
    rig.world.state().full = false;
    rig.writer.launch();
    rig.scanner.take();
    rig.world.state().full = true;
    quiet(&mut rig, Duration::from_secs(90));
    assert_eq!(lock(&rig.writer.shared.status).access, crate::find::view::Access::Limited);
    rig.writer.obey(Order::Access);
    assert_eq!(lock(&rig.writer.shared.status).access, crate::find::view::Access::Full, "opening Find asks");
}
