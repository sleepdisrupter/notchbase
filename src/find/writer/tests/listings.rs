use std::fs;

use super::*;
use crate::find::model::{flag, tier};

fn launched(name: &str) -> Rig {
    let mut rig = Rig::new(name);
    rig.writer.launch();
    rig.scanner.take();
    rig
}

#[test]
fn diff_applies_exactly() {
    let mut rig = launched("diff");
    let root = rig.root();
    let first = [file("a.txt", 10), file("b.txt", 10), folder("Docs"), file("same.txt", 10)];
    rig.listed(vec![listing(&root, &first, 1)]);
    let second = [file("a.txt", 20), ("b.txt", raw::HIDDEN, 10), file("c.txt", 5), file("same.txt", 10)];
    rig.listed(vec![listing(&root, &second, 1)]);
    let read = |rig: &Rig, path: &str| {
        let live = rig.writer.shared.reading();
        rig.find(path).map(|id| (live.modified(id), live.flags(id) & (flag::HIDDEN | flag::DIR)))
    };
    assert_eq!(read(&rig, "a.txt"), Some((20, 0)), "touched");
    assert_eq!(read(&rig, "b.txt"), Some((10, flag::HIDDEN)), "reflagged");
    assert_eq!((read(&rig, "c.txt"), read(&rig, "same.txt")), (Some((5, 0)), Some((10, 0))));
    assert_eq!(read(&rig, "Docs"), None, "buried");
    assert_eq!(rig.writer.shared.reading().alive(), 5, "the root and four names");
    assert_eq!(*lock(&rig.scanner.landed), 8, "every row is reported back to the scanner");
}

#[test]
fn file_turned_folder_is_relisted() {
    let mut rig = launched("identity");
    let root = rig.root();
    rig.listed(vec![listing(&root, &[file("build", 10)], 1)]);
    rig.listed(vec![listing(&root, &[folder("build")], 1)]);
    let live = rig.writer.shared.reading();
    let after = rig.find("build").expect("the folder");
    assert_eq!((live.flags(after) & flag::DIR, live.alive()), (flag::DIR, 2));
}

#[test]
fn incomplete_listing_buries_nothing() {
    let mut rig = launched("incomplete");
    let root = rig.root();
    rig.listed(vec![listing(&root, &[file("a.txt", 1), file("b.txt", 1)], 1)]);
    let mut cut = listing(&root, &[file("a.txt", 2), file("c.txt", 1)], 1);
    cut.complete = false;
    rig.listed(vec![cut]);
    assert!(rig.find("a.txt").is_some() && rig.find("b.txt").is_some() && rig.find("c.txt").is_some());
    assert_eq!(rig.writer.shared.reading().modified(rig.find("a.txt").unwrap()), 2, "what was read still counts");
}

#[test]
fn buried_parents_listing_is_dropped() {
    let mut rig = launched("buried-parent");
    let root = rig.root();
    rig.listed(vec![listing(&root, &[folder("Old")], 1), listing(&rig.at("Old"), &[file("x.txt", 1)], 1)]);
    rig.listed(vec![listing(&root, &[], 1)]);
    let alive = rig.writer.shared.reading().alive();
    rig.listed(vec![listing(&rig.at("Old"), &[file("y.txt", 1)], 1)]);
    rig.listed(vec![listing(&rig.at("Never"), &[file("z.txt", 1)], 1)]);
    assert_eq!(rig.writer.shared.reading().alive(), alive);
    assert!(rig.find("Old/y.txt").is_none() && rig.find("Old/x.txt").is_none() && rig.find("Never").is_none());
}

#[test]
fn shallow_find_is_descended() {
    let mut rig = launched("descend");
    let root = rig.root();
    rig.listed(vec![listing(&root, &[file("a.txt", 1)], 1)]);
    rig.events(vec![(Change::Relist(root.clone()), 40)]);
    rig.settle();
    let shallow = rig.scanner.take();
    assert_eq!(
        shallow.iter().map(|job| (job.dir.clone(), job.deep, job.live)).collect::<Vec<_>>(),
        [(root.clone(), false, true)]
    );
    let mut found = listing(&root, &[file("a.txt", 1), folder("Photos"), folder("Notes")], shallow[0].ticket);
    found.deep = false;
    rig.listed(vec![found]);
    let deeper = rig.scanner.take();
    let dirs: Vec<(PathBuf, bool, bool)> = deeper.iter().map(|job| (job.dir.clone(), job.deep, job.live)).collect();
    assert_eq!(dirs, [(rig.at("Photos"), true, true), (rig.at("Notes"), true, true)]);
    rig.finish(&shallow);
    assert_eq!(rig.cursor(), 39, "the folders it found have not been listed yet");
    rig.finish(&deeper);
    assert_eq!(rig.cursor(), 40);
}

#[test]
fn new_cargo_toml_reflags_target() {
    let mut rig = launched("cargo");
    for dir in ["proj/src", "proj/target/debug"] {
        fs::create_dir_all(rig.at(dir)).unwrap();
    }
    let (proj, target) = (rig.at("proj"), rig.at("proj/target"));
    let tier_of = |rig: &Rig, dir: &Path| rig.writer.plan.inside(dir);
    let mut inside = listing(&target, &[folder("debug"), file("a.o", 1)], 1);
    inside.tier = tier_of(&rig, &target);
    let deeper = listing(&rig.at("proj/target/debug"), &[file("x.o", 1)], 1);
    rig.listed(vec![listing(&rig.root(), &[folder("proj")], 1), listing(&proj, &[folder("src"), folder("target")], 1)]);
    rig.listed(vec![inside.clone(), deeper]);
    assert_eq!(tier(rig.writer.shared.reading().flags(rig.find("proj/target/a.o").unwrap())), 0);

    fs::write(rig.at("proj/Cargo.toml"), "[package]").unwrap();
    inside.tier = tier_of(&rig, &target);
    assert_eq!(inside.tier, 2, "a manifest makes target a build folder");
    let ids =
        ["proj/target", "proj/target/a.o", "proj/target/debug", "proj/target/debug/x.o"].map(|path| rig.find(path));
    rig.listed(vec![inside]);
    let live = rig.writer.shared.reading();
    let tiers = ids.map(|id| tier(live.flags(id.expect("still findable"))));
    assert_eq!(tiers, [0, 2, 2, 0], "the folder keeps its place, its insides sink, deeper rows wait for the next pass");
    assert!(ids.iter().all(|id| id.is_some_and(|id| !live.is_dead(id))));
}

#[test]
fn hard_stop_still_buries() {
    let mut rig = launched("hard-stop");
    let root = rig.root();
    rig.listed(vec![listing(&root, &[file("a.txt", 1), file("b.txt", 1)], 1)]);
    rig.writer.meta.level = 5;
    rig.listed(vec![listing(&root, &[file("a.txt", 3), file("new.txt", 1)], 1)]);
    assert!(rig.find("new.txt").is_none() && rig.find("b.txt").is_none());
    assert_eq!(rig.writer.shared.reading().modified(rig.find("a.txt").unwrap()), 3);
}
