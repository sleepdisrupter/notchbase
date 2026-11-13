use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::config::FindConfig;
use crate::find::os::raw;
use crate::find::shared::Order;

use super::super::{Job, Submit};
use super::{DIR, Folder, HOME, Rig, Tree, deep, listing_of, listings, names, plan, planned};

const FAN: usize = 5;
const DEPTH: usize = 4;
const SETTLE: Duration = Duration::from_secs(10);
const CLOUD: u8 = DIR | raw::DATALESS | raw::CLOUD;

fn rig(folders: &[Folder<'_>], full_access: bool) -> Rig {
    let rig = Rig::new(Tree::new(folders));
    rig.scanner.follow(Arc::new(plan(full_access)), 0);
    rig
}

fn home(below: &str) -> String {
    format!("{HOME}/{below}")
}

fn opened(rig: &Rig) -> Vec<String> {
    let below_home = |path: PathBuf| path.strip_prefix(HOME).map(|below| below.to_string_lossy().into_owned());
    rig.tree.opened().into_iter().filter_map(|path| below_home(path).ok()).collect()
}

fn rest(rig: &Rig) {
    let started = Instant::now();
    while rig.scanner.busy() && started.elapsed() < SETTLE {
        thread::sleep(Duration::from_millis(1));
    }
    assert!(!rig.scanner.busy(), "the pool goes quiet once its tickets are finished");
}

fn grown() -> Tree {
    let mut level = vec![HOME.to_owned()];
    let mut folders: Vec<(String, Vec<(String, u8)>)> = Vec::new();
    for _ in 0..DEPTH {
        let mut below = Vec::new();
        for dir in &level {
            let children: Vec<(String, u8)> = (0..FAN).map(|index| (format!("d{index}"), DIR)).collect();
            below.extend(children.iter().map(|(name, _)| format!("{dir}/{name}")));
            folders.push((dir.clone(), children.into_iter().chain([("file.txt".to_owned(), 0)]).collect()));
        }
        level = below;
    }
    let borrowed: Vec<Vec<(&str, u8)>> = folders
        .iter()
        .map(|(_, entries)| entries.iter().map(|(name, bits)| (name.as_str(), *bits)).collect())
        .collect();
    let listed: Vec<Folder<'_>> =
        folders.iter().zip(&borrowed).map(|((dir, _), entries)| (dir.as_str(), &entries[..])).collect();
    Tree::new(&listed)
}

#[test]
fn parent_listed_before_children() {
    let rig = Rig::new(grown());
    rig.scanner.follow(Arc::new(plan(true)), 0);
    rig.scanner.submit(vec![deep(HOME, 7)]);
    let orders = rig.finish(&[7]);
    let listed = listings(&orders);
    let folders = (0..=DEPTH).map(|depth| FAN.pow(depth as u32)).sum::<usize>();
    assert_eq!(listed.len(), folders, "every folder exactly once");
    for (index, listing) in listed.iter().enumerate().skip(1) {
        let parent = listing.dir.parent().expect("a folder below the first");
        assert!(
            listed[..index].iter().any(|earlier| earlier.dir == parent),
            "{:?} came before its parent",
            listing.dir
        );
    }
    assert!(matches!(orders.last(), Some(Order::Finished(7))));
    rest(&rig);
    assert!(rig.inbox.try_recv().is_err(), "nothing follows a finished ticket");
    assert!(listed.iter().all(|listing| listing.complete && listing.deep && listing.ticket == 7 && listing.tier == 0));
}

#[test]
fn guarded_media_cloud_stay_shut() {
    let folders: &[Folder<'_>] = &[
        (HOME, &[("Documents", DIR), ("Pictures", DIR), ("iCloud", CLOUD), ("notes.txt", 0)]),
        (&home("Documents"), &[("taxes.pdf", 0)]),
        (&home("Pictures"), &[("Photos Library.photoslibrary", DIR), ("cat.jpg", 0)]),
        (&home("Pictures/Photos Library.photoslibrary"), &[("database", DIR)]),
        (&home("iCloud"), &[("never-listed.txt", 0)]),
    ];
    let limited = rig(folders, false);
    limited.scanner.submit(vec![deep(HOME, 1)]);
    let orders = limited.finish(&[1]);
    assert_eq!(names(listing_of(&orders, HOME)), ["Documents", "Pictures", "iCloud", "notes.txt"]);
    assert_eq!(names(listing_of(&orders, &home("Pictures"))), ["Photos Library.photoslibrary", "cat.jpg"]);
    assert_eq!(opened(&limited), ["", "Pictures"]);

    let full = rig(folders, true);
    full.scanner.submit(vec![deep(HOME, 2)]);
    let orders = full.finish(&[2]);
    let library = "Pictures/Photos Library.photoslibrary";
    let database = format!("{library}/database");
    assert_eq!(opened(&full), ["", "Documents", "Pictures", library, database.as_str()]);
    let inside = listing_of(&orders, &home(library));
    assert!(inside.inside_package && inside.tier == 1, "a library's insides are package insides");
    assert!(!listing_of(&orders, HOME).inside_package);
}

#[test]
fn other_device_and_link_are_leaves() {
    let folders: &[Folder<'_>] = &[
        (HOME, &[("mounted", DIR | raw::ELSEWHERE), ("to-music", raw::LINK), ("Music", DIR)]),
        (&home("mounted"), &[("other-disk.txt", 0)]),
        (&home("to-music"), &[("loop", raw::LINK)]),
    ];
    let rig = rig(folders, true);
    rig.scanner.submit(vec![deep(HOME, 1)]);
    let orders = rig.finish(&[1]);
    assert_eq!(names(listing_of(&orders, HOME)), ["mounted", "to-music", "Music"]);
    assert_eq!(opened(&rig), ["", "Music"]);
}

#[test]
fn unwanted_job_still_finishes() {
    let folders: &[Folder<'_>] = &[(HOME, &[("Documents", DIR), ("Music", DIR)]), (&home("Documents"), &[("a", 0)])];
    let rig = rig(folders, false);
    let unwanted = ["/disk/Users/me/Documents", "/disk/System/Library", "/elsewhere", "/disk/Volumes/Work"];
    let jobs: Vec<Job> = unwanted.iter().zip(1..).map(|(dir, ticket)| deep(dir, ticket)).collect();
    rig.scanner.submit(jobs.into_iter().chain([deep(&home("Music"), 9), deep(&home("Documents/deeper"), 9)]).collect());
    let orders = rig.finish(&[1, 2, 3, 4, 9]);
    assert_eq!(listings(&orders).len(), 1);
    assert_eq!(opened(&rig), ["Music"], "nothing unwanted was opened, by any ticket");
}

#[test]
fn reach_and_depth_limit_a_pass() {
    let folders: &[Folder<'_>] = &[
        (HOME, &[("app", DIR), ("Library", DIR)]),
        (&home("app"), &[("node_modules", DIR), ("src", DIR)]),
        (&home("app/node_modules"), &[("left-pad", DIR)]),
        (&home("Library"), &[("Caches", DIR), ("Preferences", DIR)]),
    ];
    let rig = rig(folders, true);
    rig.scanner.submit(vec![Job { reach: 1, ..deep(HOME, 1) }]);
    let orders = rig.finish(&[1]);
    assert_eq!(opened(&rig), ["", "Library", "Library/Preferences", "app", "app/src"]);
    assert_eq!(names(listing_of(&orders, &home("app"))), ["node_modules", "src"], "skipped folders are still rows");
    assert!(listings(&orders).iter().all(|listing| listing.reach == 1 && listing.tier <= 1));

    let shallow = Rig::new(Tree::new(folders));
    shallow.scanner.follow(Arc::new(plan(true)), 0);
    shallow.scanner.submit(vec![Job { deep: false, live: true, ..deep(&home("app/node_modules"), 2) }]);
    let orders = shallow.finish(&[2]);
    assert_eq!(opened(&shallow), ["app/node_modules"]);
    let listing = listing_of(&orders, &home("app/node_modules"));
    assert!(!listing.deep && listing.tier == 2, "the scanner works out the tier of a folder it is handed");
}

#[test]
fn unreadable_or_missing_folder() {
    let folders: &[Folder<'_>] =
        &[(HOME, &[("Mail", DIR), ("gone", DIR), ("Music", DIR)]), (&home("Mail"), &[("unseen", 0)])];
    let rig = Rig::new(Tree::new(folders).locked(&home("Mail")).without(&home("gone")));
    rig.scanner.follow(Arc::new(plan(true)), 0);
    rig.scanner.submit(vec![deep(HOME, 4)]);
    let orders = rig.finish(&[4]);
    let mail = listing_of(&orders, &home("Mail"));
    assert!(!mail.complete && mail.rows.is_empty(), "the writer must bury nothing under a folder it could not read");
    assert!(listing_of(&orders, HOME).complete && listing_of(&orders, &home("Music")).complete);
    assert!(listings(&orders).iter().all(|listing| listing.dir != Path::new(&home("gone"))));
    assert_eq!(opened(&rig), ["", "Mail", "Music", "gone"]);
}

#[test]
fn level_holds_back_and_excludes() {
    let folders: &[Folder<'_>] = &[
        (HOME, &[("app", DIR), ("Library", DIR), ("Private", DIR), ("secret", DIR), ("secret.txt", 0)]),
        (&home("app"), &[("node_modules", DIR), ("secret", 0)]),
        (&home("Library"), &[("Caches", DIR), ("Application Support", DIR)]),
        (&home("Library/Application Support"), &[("notchbase", DIR)]),
        (&home("Library/Application Support/notchbase"), &[("find", DIR), ("config.json", 0)]),
    ];
    let rig = Rig::new(Tree::new(folders));
    let config = FindConfig { excluded: vec!["secret".into(), "~/Private".into()], ..FindConfig::default() };
    rig.scanner.follow(Arc::new(planned(&config, true)), 2);
    rig.scanner.submit(vec![deep(HOME, 1), deep(&home("Library/Caches"), 2)]);
    let orders = rig.finish(&[1, 2]);
    assert_eq!(names(listing_of(&orders, HOME)), ["app", "Library", "secret.txt"]);
    assert_eq!(names(listing_of(&orders, &home("app"))), ["node_modules"]);
    assert_eq!(names(listing_of(&orders, &home("Library/Application Support/notchbase"))), ["config.json"]);
    let support = "Library/Application Support";
    let ours = format!("{support}/notchbase");
    assert_eq!(opened(&rig), ["", "Library", support, ours.as_str(), "app"]);
}

#[test]
fn pool_comes_back_for_next_errand() {
    let folders: &[Folder<'_>] = &[(HOME, &[("Music", DIR)]), (&home("Music"), &[("a.mp3", 0)])];
    let rig = rig(folders, true);
    assert!(!rig.scanner.busy());
    for ticket in 1..=3 {
        rig.scanner.submit(vec![Job { live: ticket == 2, ..deep(HOME, ticket) }]);
        let orders = rig.finish(&[ticket]);
        assert_eq!(listings(&orders).len(), 2);
        rest(&rig);
    }
    rig.scanner.landed(usize::MAX);
    rig.scanner.submit(Vec::new());
    assert!(!rig.scanner.busy() && rig.inbox.try_recv().is_err());
}
