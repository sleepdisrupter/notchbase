use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::*;
use crate::config::FindConfig;
use crate::find::scan::Fences;

const HOME: &str = "/disk/Users/me";
const DATA: &str = "/disk/Users/me/Library/notchbase/find";

fn open() -> Plan {
    Plan::fenced(&FindConfig::default(), &[], Some(Path::new(HOME)), Some(Path::new(DATA)), true, Fences::default())
}

fn limited() -> Plan {
    let guarded = vec![Path::new(HOME).join("Documents")];
    let fences = Fences { guarded, ..Fences::default() };
    Plan::fenced(&FindConfig::default(), &[], Some(Path::new(HOME)), Some(Path::new(DATA)), false, fences)
}

fn at(path: &str) -> PathBuf {
    Path::new(HOME).join(path)
}

fn touched(path: &str, id: u64) -> Event {
    Event { change: Change::Touched(at(path)), id }
}

fn after(start: Instant, millis: u64) -> Instant {
    start + Duration::from_millis(millis)
}

#[test]
fn nothing_leaves_unsettled() {
    let (mut coalescer, plan, start) = (Coalescer::new(), open(), Instant::now());
    coalescer.push(touched("a.txt", 7), &plan, start);
    assert!(coalescer.due(after(start, 249), false).is_empty());
    assert!(!coalescer.is_empty());
    assert_eq!(coalescer.due(after(start, 250), false), [touched("a.txt", 7)]);
    assert!(coalescer.is_empty() && coalescer.oldest().is_none());
}

#[test]
fn merge_keeps_lowest_id() {
    let (mut coalescer, plan, start) = (Coalescer::new(), open(), Instant::now());
    coalescer.push(touched("a.txt", 9), &plan, start);
    coalescer.push(touched("a.txt", 4), &plan, after(start, 100));
    coalescer.push(touched("a.txt", 12), &plan, after(start, 200));
    assert_eq!(coalescer.oldest(), Some(4));
    assert_eq!(coalescer.due(after(start, 250), false), [touched("a.txt", 4)], "a file saved again and again");
}

#[test]
fn many_children_become_a_relist() {
    let (mut coalescer, plan, start) = (Coalescer::new(), open(), Instant::now());
    (0..32).for_each(|n| coalescer.push(touched(&format!("shots/{n}.png"), 100 + n), &plan, start));
    coalescer.push(touched("other.txt", 90), &plan, start);
    assert_eq!(coalescer.due(after(start, 250), false).len(), 33, "thirty-two are still looked at one by one");
    (0..33).for_each(|n| coalescer.push(touched(&format!("shots/{n}.png"), 200 + n), &plan, start));
    coalescer.push(touched("shots/late.png", 150), &plan, start);
    let relist = Event { change: Change::Relist(at("shots")), id: 150 };
    assert_eq!(coalescer.due(after(start, 250), false), [relist]);
}

#[test]
fn pending_subtree_absorbs_below() {
    let (mut coalescer, plan, start) = (Coalescer::new(), open(), Instant::now());
    coalescer.push(Event { change: Change::Subtree(at("project")), id: 50 }, &plan, start);
    coalescer.push(touched("project/src/main.rs", 40), &plan, start);
    coalescer.push(Event { change: Change::Relist(at("project/src")), id: 45 }, &plan, start);
    coalescer.push(touched("project", 60), &plan, start);
    let due = coalescer.due(after(start, 250), false);
    let subtree = Event { change: Change::Subtree(at("project")), id: 40 };
    assert_eq!(due, [subtree, touched("project", 60)], "the folder's own row is its parent's business");
}

#[test]
fn unwanted_path_is_dropped() {
    let (mut coalescer, plan, start) = (Coalescer::new(), limited(), Instant::now());
    coalescer.push(touched("Documents/taxes.pdf", 1), &plan, start);
    coalescer.push(touched("Documents", 2), &plan, start);
    coalescer.push(Event { change: Change::Touched(Path::new(DATA).join("index.bin")), id: 3 }, &plan, start);
    coalescer.push(Event { change: Change::Relist(at("Documents/inbox")), id: 4 }, &plan, start);
    coalescer.push(touched("notes.txt", 5), &plan, start);
    assert_eq!(coalescer.due(after(start, 250), false), [touched("notes.txt", 5)]);
    assert!(coalescer.is_empty(), "nothing guarded was kept for later either");
}

#[test]
fn move_across_fence_seen_inside() {
    let (mut coalescer, plan, start) = (Coalescer::new(), limited(), Instant::now());
    let moved = |from: &str, to: &str, id| Event { change: Change::Moved { from: at(from), to: at(to) }, id };
    coalescer.push(moved("Documents/a.pdf", "a.pdf", 1), &plan, start);
    coalescer.push(moved("b.pdf", "Documents/b.pdf", 2), &plan, start);
    coalescer.push(moved("Documents/c.pdf", "Documents/d.pdf", 3), &plan, start);
    coalescer.push(moved("e.pdf", "f.pdf", 4), &plan, start);
    let due = coalescer.due(after(start, 250), false);
    assert_eq!(due, [touched("a.pdf", 1), touched("b.pdf", 2), moved("e.pdf", "f.pdf", 4)]);
}

#[test]
fn noisy_path_waits_longer() {
    let (mut coalescer, plan, start) = (Coalescer::new(), open(), Instant::now());
    coalescer.push(touched("app/node_modules/left-pad/index.js", 30), &plan, start);
    coalescer.push(touched("app/src/index.js", 31), &plan, after(start, 1_000));
    assert_eq!(coalescer.due(after(start, 1_250), false), [touched("app/src/index.js", 31)]);
    assert_eq!(coalescer.oldest(), Some(30), "the build storm is still owed");
    assert!(coalescer.due(after(start, 4_999), false).is_empty());
    assert_eq!(coalescer.due(after(start, 5_000), false), [touched("app/node_modules/left-pad/index.js", 30)]);
}

#[test]
fn battery_doubles_slow_wait() {
    let (mut coalescer, plan, start) = (Coalescer::new(), open(), Instant::now());
    coalescer.push(touched("rust/target/debug/build.log", 8), &plan, start);
    assert!(coalescer.due(after(start, 9_999), true).is_empty());
    assert_eq!(coalescer.due(after(start, 10_000), true).len(), 1);
}

#[test]
fn build_storm_is_one_relist() {
    let (mut coalescer, plan, start) = (Coalescer::new(), open(), Instant::now());
    let objects = (0..5_000).map(|n| touched(&format!("rust/target/debug/deps/{n}.o"), 1_000 + n));
    objects.for_each(|event| coalescer.push(event, &plan, start));
    let due = coalescer.due(after(start, 5_000), false);
    assert_eq!(due, [Event { change: Change::Relist(at("rust/target/debug/deps")), id: 1_000 }]);
}

#[test]
fn volume_changes_never_wait() {
    let (mut coalescer, plan, start) = (Coalescer::new(), limited(), Instant::now());
    let disk = PathBuf::from("/Volumes/Archive");
    coalescer.push(Event { change: Change::Everything(disk.clone()), id: 2 }, &plan, start);
    coalescer.push(Event { change: Change::Overflow(disk.clone()), id: 3 }, &plan, start);
    coalescer.push(Event { change: Change::Overflow(disk.clone()), id: 1 }, &plan, start);
    let due = coalescer.due(after(start, 250), false);
    assert_eq!(
        due,
        [Event { change: Change::Overflow(disk.clone()), id: 1 }, Event { change: Change::Everything(disk), id: 2 }]
    );
}

#[test]
fn own_saves_are_not_changes() {
    let (mut coalescer, plan, start) = (Coalescer::new(), open(), Instant::now());
    let data = |name: &str| Path::new(DATA).join(name);
    let own = ["index.bin", "index.bin.tmp", "usage.bin", "usage.bin.tmp", "icons", "icons/probe", "icons/probe.pdf"];
    let mut id = 0;
    let mut push = |change: Change| {
        id += 1;
        coalescer.push(Event { change, id }, &plan, start);
    };
    for _ in 0..40 {
        own.iter().for_each(|name| push(Change::Touched(data(name))));
    }
    push(Change::Touched(PathBuf::from(DATA)));
    push(Change::Relist(PathBuf::from(DATA)));
    push(Change::Subtree(data("icons")));
    push(Change::Moved { from: data("index.bin.tmp"), to: data("index.bin") });
    push(Change::Moved { from: data("usage.bin.tmp"), to: data("usage.bin") });
    assert!(coalescer.is_empty() && coalescer.due(after(start, 60_000), false).is_empty());
}
