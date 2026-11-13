#![cfg(unix)]

use std::collections::BTreeMap;
use std::fs::{self, Permissions};
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _, symlink};
use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::time::Instant;

use crate::config::FindConfig;
use crate::find::os::{self, Listed, Root, raw, root};
use crate::find::shared::{Order, Shared};
use crate::find::tests::Scratch;
use crate::find::usage::Usage;

use super::super::{Fences, Job, Plan, Scanner, Submit};
use super::{finish, listings, names};

const LOCKED: u32 = 0o000;
const OPEN: u32 = 0o755;
const GROUPS: usize = 100;
const FOLDERS_EACH: usize = 200;
const FILES_EACH: usize = 5;

struct Disk {
    tree: Scratch,
    scanner: Scanner,
    inbox: Receiver<Order>,
}

impl Disk {
    fn new(name: &str) -> Self {
        let (sink, inbox) = mpsc::channel();
        let shared = Arc::new(Shared::new(Usage::load(None, 0), Arc::new(|| {})));
        Self { tree: Scratch::new(name), scanner: Scanner::new(shared, sink), inbox }
    }

    fn plant(&self, folders: &[&str], files: &[&str]) {
        folders.iter().for_each(|folder| fs::create_dir_all(self.tree.file(folder)).expect("a folder"));
        files.iter().for_each(|file| fs::write(self.tree.file(file), b"").expect("a file"));
    }

    fn follow(&self, excluded: &[&str], devices: [u64; 2]) {
        let config =
            FindConfig { excluded: excluded.iter().map(|name| (*name).to_owned()).collect(), ..FindConfig::default() };
        let root = Root { path: self.tree.dir().to_path_buf(), volume: [0; 16], devices, flags: root::BOOT };
        let plan = Plan::fenced(&config, &[root], Some(self.tree.dir()), None, true, Fences::default());
        self.scanner.follow(Arc::new(plan), 0);
    }

    fn device(&self) -> u64 {
        fs::metadata(self.tree.dir()).expect("the tree").dev()
    }

    fn walk(&self, ticket: u32) -> BTreeMap<String, (Vec<String>, u8, bool)> {
        let dir = self.tree.dir().to_path_buf();
        self.scanner.submit(vec![Job { dir, tier: 0, deep: true, reach: 3, ticket, live: false }]);
        let orders = finish(&self.inbox, &[ticket]);
        let below = |dir: &Path| {
            dir.strip_prefix(self.tree.dir()).expect("a folder in the tree").to_string_lossy().into_owned()
        };
        let sorted = |mut names: Vec<String>| {
            names.sort();
            names
        };
        let listed = listings(&orders).into_iter();
        listed.map(|listing| (below(&listing.dir), (sorted(names(listing)), listing.tier, listing.complete))).collect()
    }
}

#[test]
fn real_tree_walked_once() {
    let disk = Disk::new("scan-loops");
    disk.plant(
        &["Music/Albums", "skip-me/inside", "Projects/app/skip-me"],
        &["Music/Albums/a.mp3", "Projects/app/main.rs"],
    );
    symlink(disk.tree.dir(), disk.tree.file("Music/up")).expect("a link back to the top");
    symlink(disk.tree.file("Music"), disk.tree.file("Music/Albums/around")).expect("a link to a parent");
    symlink(disk.tree.file("missing"), disk.tree.file("dangling")).expect("a link to nothing");
    disk.follow(&["skip-me"], [disk.device(); 2]);
    let walked = disk.walk(1);
    let folders: Vec<&str> = walked.keys().map(String::as_str).collect();
    assert_eq!(
        folders,
        ["", "Music", "Music/Albums", "Projects", "Projects/app"],
        "each folder once, links never entered"
    );
    assert_eq!(walked[""].0, ["Music", "Projects", "dangling"]);
    assert_eq!(walked["Music"].0, ["Albums", "up"]);
    assert_eq!(walked["Music/Albums"].0, ["a.mp3", "around"]);
    assert_eq!(walked["Projects/app"].0, ["main.rs"]);
    assert!(walked.values().all(|(_, tier, complete)| *tier == 0 && *complete));
}

#[test]
fn target_beside_cargo_is_a_build() {
    let disk = Disk::new("scan-target");
    disk.plant(&["crate/target/debug", "notes/target/inner", "crate/node_modules/dep"], &["crate/Cargo.toml"]);
    disk.follow(&[], [disk.device(); 2]);
    let walked = disk.walk(1);
    let tier = |folder: &str| walked[folder].1;
    assert_eq!(
        (tier("crate"), tier("crate/target"), tier("crate/target/debug"), tier("crate/node_modules")),
        (0, 2, 2, 2)
    );
    assert_eq!((tier("notes"), tier("notes/target"), tier("notes/target/inner")), (0, 0, 0));
    assert_eq!(walked["crate"].0, ["Cargo.toml", "node_modules", "target"], "the build folder itself is a row");
}

#[test]
fn unopenable_folder_is_incomplete() {
    let disk = Disk::new("scan-locked");
    disk.plant(&["open/inner", "locked/inner"], &["locked/unseen.txt"]);
    fs::set_permissions(disk.tree.file("locked"), Permissions::from_mode(LOCKED)).expect("a locked folder");
    disk.follow(&[], [disk.device(); 2]);
    let walked = disk.walk(1);
    fs::set_permissions(disk.tree.file("locked"), Permissions::from_mode(OPEN)).expect("an unlocked folder");
    assert_eq!(walked["locked"], (vec![], 0, false));
    assert!(walked[""].2 && walked["open"].2 && walked["open/inner"].2);
    assert!(!walked.contains_key("locked/inner"));
}

#[test]
fn device_outside_root_is_a_leaf() {
    let disk = Disk::new("scan-devices");
    disk.plant(&["other-volume/inside"], &["here.txt"]);
    let elsewhere = disk.device() + 1;
    disk.follow(&[], [elsewhere, elsewhere + 1]);
    disk.scanner.submit(vec![Job {
        dir: disk.tree.dir().to_path_buf(),
        tier: 0,
        deep: true,
        reach: 3,
        ticket: 5,
        live: false,
    }]);
    let orders = finish(&disk.inbox, &[5]);
    let listed = listings(&orders);
    assert_eq!(listed.len(), 1, "the walk stays on the root's devices");
    let mut top = names(listed[0]);
    top.sort();
    assert_eq!(top, ["here.txt", "other-volume"]);
    assert!(listed[0].rows.iter().any(|row| row.bits == raw::DIR | raw::ELSEWHERE));
}

#[test]
fn guarded_manifest_not_looked_for() {
    let disk = Disk::new("scan-guarded-manifest");
    disk.plant(&["Documents/crate/target"], &["Documents/crate/Cargo.toml"]);
    let roots = [Root { path: disk.tree.dir().to_path_buf(), volume: [0; 16], devices: [0, 0], flags: root::BOOT }];
    let fences = || Fences { guarded: vec![disk.tree.file("Documents")], ..Fences::default() };
    let home = Some(disk.tree.dir());
    let plan = |full_access| Plan::fenced(&FindConfig::default(), &roots, home, None, full_access, fences());
    let target = disk.tree.file("Documents/crate/target");
    assert_eq!(plan(true).inside(&target), 2);
    assert_eq!(plan(false).inside(&target), 0, "nothing below a guarded folder is touched, not even to ask its tier");
}

#[test]
#[ignore = "prints how fast a hundred thousand files in a temporary tree are listed"]
fn large_tree_lists_at_bulk_speed() {
    let disk = Disk::new("scan-rate");
    let groups = (0..GROUPS).flat_map(|group| (0..FOLDERS_EACH).map(move |folder| format!("g{group:02}/f{folder:03}")));
    let folders: Vec<String> = groups.collect();
    for folder in &folders {
        fs::create_dir_all(disk.tree.file(folder)).expect("a folder");
        let files = (0..FILES_EACH).map(|file| disk.tree.file(&format!("{folder}/file-{file}.txt")));
        files.for_each(|file| fs::write(file, b"").expect("a file"));
    }
    let bulk = || {
        let (started, mut listed) = (Instant::now(), Listed::default());
        let rows = folders
            .iter()
            .map(|folder| os::list(&disk.tree.file(folder), [0, 0], &mut listed).map(|()| listed.rows.len()));
        (rows.map(|rows| rows.expect("a listing")).sum::<usize>(), started.elapsed())
    };
    let plain = || {
        let started = Instant::now();
        let stats = folders.iter().flat_map(|folder| fs::read_dir(disk.tree.file(folder)).expect("a folder"));
        (stats.filter(|entry| entry.as_ref().is_ok_and(|entry| entry.metadata().is_ok())).count(), started.elapsed())
    };
    let (cold, standard, warm) = (bulk(), plain(), bulk());
    disk.follow(&[], [disk.device(); 2]);
    let started = Instant::now();
    let walked = disk.walk(1);
    let pool = (walked.values().map(|(names, _, _)| names.len()).sum::<usize>(), started.elapsed());
    for (what, (rows, took)) in
        [("bulk, first", cold), ("std", standard), ("bulk, again", warm), ("four workers", pool)]
    {
        eprintln!("{what}: {rows} rows in {took:?}, {:.0} rows a second", rows as f64 / took.as_secs_f64());
    }
    let files = GROUPS * FOLDERS_EACH * FILES_EACH;
    assert_eq!((cold.0, standard.0, warm.0), (files, files, files));
    assert_eq!(pool.0, files + GROUPS * FOLDERS_EACH + GROUPS);
}
