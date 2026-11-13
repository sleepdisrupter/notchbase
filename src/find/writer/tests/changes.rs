use std::fs;

use super::*;
use crate::find::store::mark;

fn launched(name: &str) -> Rig {
    let mut rig = Rig::new(name);
    rig.writer.launch();
    rig.scanner.take();
    rig
}

fn shape(jobs: &[Job]) -> Vec<(PathBuf, bool, u8)> {
    jobs.iter().map(|job| (job.dir.clone(), job.deep, job.reach)).collect()
}

#[test]
fn unknown_path_relists_parent() {
    let mut rig = launched("touched-miss");
    let root = rig.root();
    rig.listed(vec![listing(&root, &[folder("Docs")], 1), listing(&rig.at("Docs"), &[], 1)]);
    let alive = rig.writer.shared.reading().alive();
    rig.events(vec![(Change::Touched(rig.at("Docs/new.txt")), 5), (Change::Touched(rig.at("Far/Away/x.txt")), 6)]);
    rig.settle();
    assert_eq!(rig.writer.shared.reading().alive(), alive);
    assert_eq!(shape(&rig.scanner.take()), [(rig.at("Docs"), false, 3), (root, false, 3)]);
}

#[test]
fn touched_file_buried_or_kept() {
    let mut rig = launched("touched-hit");
    fs::write(rig.at("a.txt"), "kept").unwrap();
    rig.listed(vec![listing(&rig.root(), &[file("a.txt", 1), file("b.txt", 1)], 1)]);
    rig.events(vec![(Change::Touched(rig.at("a.txt")), 1), (Change::Touched(rig.at("b.txt")), 2)]);
    rig.settle();
    let on_disk = os::stat(&rig.at("a.txt")).unwrap().unwrap().modified;
    let found = rig.find("a.txt").expect("still there");
    assert_eq!(rig.writer.shared.reading().modified(found), on_disk);
    assert!(rig.find("b.txt").is_none(), "gone from the disk, gone from the index");
    assert!(rig.scanner.take().is_empty(), "a hit needs no listing");
}

#[test]
fn renamed_folder_stays_findable() {
    let mut rig = launched("renamed");
    let root = rig.root();
    fs::create_dir_all(rig.at("Old")).unwrap();
    fs::write(rig.at("Old/x.txt"), "x").unwrap();
    rig.listed(vec![listing(&root, &[folder("Old")], 1), listing(&rig.at("Old"), &[file("x.txt", 1)], 1)]);
    fs::rename(rig.at("Old"), rig.at("New")).unwrap();
    rig.events(vec![(Change::Moved { from: rig.at("Old"), to: rig.at("New") }, 50)]);
    rig.settle();
    let parent = rig.scanner.take();
    assert_eq!(shape(&parent), [(root.clone(), false, 3)]);
    let mut shallow = listing(&root, &[folder("New")], parent[0].ticket);
    shallow.deep = false;
    rig.listed(vec![shallow]);
    assert!(rig.find("Old/x.txt").is_some() && rig.find("New").is_some(), "both places answer meanwhile");
    let inside = rig.scanner.take();
    rig.listed(vec![listing(&rig.at("New"), &[file("x.txt", 1)], inside[0].ticket)]);
    rig.finish(&parent);
    assert!(rig.find("Old/x.txt").is_some(), "the new place is still being listed");
    rig.finish(&inside);
    assert!(rig.find("Old").is_none() && rig.find("Old/x.txt").is_none());
    assert!(rig.find("New/x.txt").is_some());
    assert_eq!(rig.cursor(), 50);
}

#[test]
fn false_move_is_two_touches() {
    let mut rig = launched("not-moved");
    fs::write(rig.at("a.txt"), "still here").unwrap();
    rig.listed(vec![listing(&rig.root(), &[file("a.txt", 1)], 1)]);
    rig.events(vec![(Change::Moved { from: rig.at("a.txt"), to: rig.at("b.txt") }, 9)]);
    rig.settle();
    assert!(rig.find("a.txt").is_some());
    assert_eq!(shape(&rig.scanner.take()), [(rig.root(), false, 3)], "b.txt is unknown, so its folder is listed");
}

#[test]
fn overflow_storm_rescans_once() {
    let mut rig = launched("storm");
    let root = rig.root();
    rig.events(vec![(Change::Overflow(root.clone()), 10)]);
    rig.settle();
    let first = rig.scanner.take();
    assert_eq!(shape(&first), [(root.clone(), true, 1)]);
    rig.finish(&first);
    for (minutes, id) in [(1, 20), (1, 30), (7, 40)] {
        rig.world.pass(Duration::from_secs(60 * minutes));
        rig.events(vec![(Change::Overflow(root.clone()), id)]);
        rig.settle();
    }
    assert!(rig.scanner.take().is_empty(), "inside the gap the storm only queues one more pass");
    assert_eq!(rig.cursor(), 19);
    rig.world.pass(Duration::from_secs(60));
    rig.writer.tick();
    let second = rig.scanner.take();
    assert_eq!(shape(&second), [(root, true, 1)]);
    rig.finish(&second);
    assert_eq!(rig.cursor(), 40);
}

#[test]
fn everything_clears_complete_mark() {
    let mut rig = launched("everything");
    let root = rig.root();
    rig.writer.mark_of(&root).unwrap().flags |= mark::COMPLETE;
    rig.events(vec![(Change::Everything(root.clone()), 3)]);
    rig.settle();
    assert_eq!(rig.writer.mark_of(&root).unwrap().flags & mark::COMPLETE, 0);
    let sweep = rig.scanner.take();
    assert_eq!(shape(&sweep), [(root.clone(), true, 3)]);
    rig.finish(&sweep);
    let swept = rig.writer.mark_of(&root).unwrap().clone();
    assert_eq!((swept.flags & mark::COMPLETE, swept.scanned_at, swept.swept_at), (mark::COMPLETE, START, START));
}

#[test]
fn mount_scans_the_new_root() {
    let mut rig = launched("mount");
    let volume = Scratch::new("mount-volume");
    let new = Root { path: volume.dir().to_path_buf(), volume: [9; 16], devices: [0, 0], flags: 0 };
    rig.world.state().roots.push(new.clone());
    rig.events(vec![(Change::Overflow(PathBuf::from("/Volumes/Camera")), 4)]);
    rig.settle();
    assert!(rig.writer.shared.reading().resolve(&new.path).is_some(), "the new root has its row");
    assert_eq!(shape(&rig.scanner.take()), [(new.path.clone(), true, 3)]);
    assert!(rig.log().iter().any(|line| line.starts_with("watch") && line.contains("mount-volume")));
}

#[test]
fn crowded_folder_relists_slowly() {
    let mut rig = launched("crowded");
    let names: Vec<String> = (0..20_001).map(|n| format!("{n}.png")).collect();
    let entries: Vec<(&str, u8, u32)> = names.iter().map(|name| file(name, 1)).collect();
    rig.listed(vec![listing(&rig.root(), &[folder("Shots")], 1), listing(&rig.at("Shots"), &entries, 1)]);
    let shots = rig.at("Shots");
    rig.events(vec![(Change::Relist(shots.clone()), 1)]);
    rig.settle();
    rig.finish(&rig.scanner.take());
    rig.events(vec![(Change::Relist(shots.clone()), 2)]);
    rig.settle();
    assert!(rig.scanner.take().is_empty());
    assert_eq!(rig.cursor(), 1, "the waiting relist still holds its event");
    rig.world.pass(Duration::from_secs(5));
    rig.writer.tick();
    assert_eq!(shape(&rig.scanner.take()), [(shots, false, 3)]);
}

#[test]
fn cursor_never_passes_pending_work() {
    let mut rig = launched("cursor");
    rig.listed(vec![listing(&rig.root(), &[folder("app")], 1)]);
    rig.events(vec![(Change::Touched(rig.at("app/node_modules/x.js")), 10), (Change::Touched(rig.at("app/y.js")), 20)]);
    rig.settle();
    let relist = rig.scanner.take();
    assert_eq!(rig.cursor(), 9, "a build storm waits in the slow lane");
    rig.world.pass(Duration::from_secs(5));
    rig.writer.tick();
    let storm = rig.scanner.take();
    assert_eq!(rig.cursor(), 9, "both errands are still out, one of them held by a paused scanner");
    rig.finish(&storm);
    assert_eq!(rig.cursor(), 19);
    rig.finish(&relist);
    assert_eq!(rig.cursor(), 20);
}

#[test]
fn reported_missing_file_is_buried() {
    let mut rig = launched("gone");
    rig.listed(vec![listing(&rig.root(), &[file("lost.txt", 1)], 1)]);
    rig.writer.obey(Order::Gone(rig.at("lost.txt")));
    assert!(rig.find("lost.txt").is_none());
}
