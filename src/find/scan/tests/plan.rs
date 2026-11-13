use std::path::{Path, PathBuf};

use crate::config::FindConfig;

use super::super::{Fences, Plan};
use super::{DISK, HOME, boot, fences, plan, planned};

const WORK: &str = "/disk/Volumes/Work";

fn home(below: &str) -> PathBuf {
    Path::new(HOME).join(below)
}

fn excluding(entries: &[&str]) -> Plan {
    let config =
        FindConfig { excluded: entries.iter().map(|entry| (*entry).to_owned()).collect(), ..FindConfig::default() };
    planned(&config, true)
}

#[test]
fn guarded_paths_unwanted_while_limited() {
    let (limited, full) = (plan(false), plan(true));
    let guarded = ["Desktop", "Documents", "Downloads", "Library/Containers"];
    for folder in guarded {
        for below in ["", "a.txt", "deep/er/still/b.txt", ".hidden/c"] {
            let path = home(folder).join(below);
            assert!(!limited.wanted(&path), "{path:?} must not be touched while limited");
            assert!(full.wanted(&path), "{path:?} is fair game with full access");
        }
    }
    for beside in ["Documents2/a.txt", "Desk", "Library/Preferences/x.plist", "Music/song.mp3", ""] {
        assert!(limited.wanted(&home(beside)), "{beside} only looks like a guarded folder");
    }
}

#[test]
fn media_and_removables_closed() {
    let (limited, full) = (plan(false), plan(true));
    let library = home("Pictures/Photos Library.photoslibrary");
    for path in
        [library.clone(), library.join("database/Photos.sqlite"), home("Music/Music/Music Library.musiclibrary")]
    {
        assert!(!limited.wanted(&path) && full.wanted(&path), "{path:?}");
    }
    for path in [WORK, "/disk/Volumes/Work/Projects/a.rs"] {
        assert!(!limited.wanted(Path::new(path)) && full.wanted(Path::new(path)), "{path}");
    }
    assert!(limited.wanted(&home("Pictures/holiday.jpg")));
}

#[test]
fn sealed_tree_open_only_to_doors() {
    let plan = plan(true);
    let wanted = |path: &str| plan.wanted(&Path::new(DISK).join(path));
    assert!(
        wanted("")
            && wanted("System")
            && wanted("System/Applications")
            && wanted("System/Applications/Maps.app/Contents")
    );
    assert!(wanted("usr") && wanted("usr/local/bin/tool"));
    assert!(
        !wanted("System/Library") && !wanted("System/Library/Fonts/a.ttf") && !wanted("usr/bin/env") && !wanted("bin")
    );
    assert!(
        !wanted("Volumes") && !wanted("Volumes/Network/share.txt"),
        "an unknown volume is not walked from the boot root"
    );
    assert!(wanted("Volumes/Work/System/Library"), "another root is not sealed by the boot root's rules");
    assert!(!plan.wanted(Path::new("/elsewhere/file")), "a path under no root is nobody's to scan");
}

#[test]
fn exclusions_and_backups_unwanted() {
    let plan = excluding(&["secret", "~/Private", "/disk/Volumes/Work/scratch"]);
    for path in [
        "Projects/secret/key.pem",
        "secret",
        "Private",
        "Private/diary.txt",
        "Library/Application Support/notchbase/find/index.bin",
    ] {
        assert!(!plan.wanted(&home(path)), "{path}");
    }
    for path in ["scratch/a", "2026-10-06-101500.previous/Users/me/a.txt", "Backups.backupdb"] {
        assert!(!plan.wanted(&Path::new(WORK).join(path)), "{path}");
    }
    for path in ["Projects/secrets/key.pem", "Privately", "Library/Application Support/notchbase/config.json"] {
        assert!(plan.wanted(&home(path)), "{path}");
    }
    assert!(plan.wanted(&home("old.previous/a.txt")), "only the top of another volume is read as a backup");
}

#[test]
fn rootless_plan_wants_all_unfenced() {
    let plan = Plan::fenced(&FindConfig::default(), &[], Some(Path::new(HOME)), None, false, fences());
    assert!(plan.wanted(Path::new("/anywhere/at/all")) && plan.wanted(&home("Music")));
    assert!(!plan.wanted(&home("Documents/a.txt")) && !plan.wanted(Path::new("/disk/bin/sh")));
    assert_eq!(plan.devices(Path::new("/anywhere")), [0, 0]);
    assert_eq!(plan.inside(&home("Library/Caches")), 3);
}

#[test]
fn no_home_limited_scans_nothing() {
    let roots = [boot(DISK)];
    let blind = Plan::fenced(&FindConfig::default(), &roots, None, None, false, fences());
    assert!(!blind.wanted(Path::new(DISK)) && !blind.wanted(&home("Music/a.mp3")), "it cannot tell what is guarded");
    let sighted = Plan::fenced(&FindConfig::default(), &roots, None, None, true, fences());
    assert!(sighted.wanted(Path::new(DISK)) && sighted.wanted(&home("Documents/a.txt")));
}

#[test]
fn levels_hold_back_tiers() {
    let plan = plan(true);
    let boot_dir = home("Projects");
    let held = |level: u8| {
        let boot: Vec<u8> = (0..4).filter(|tier| plan.held_back(level, &boot_dir, *tier)).collect();
        let other: Vec<u8> = (0..4).filter(|tier| plan.held_back(level, Path::new(WORK), *tier)).collect();
        (boot, other)
    };
    assert_eq!(held(0), (vec![], vec![]));
    assert_eq!(held(1), (vec![3], vec![3]), "caches go first, on every root");
    assert_eq!(held(2), (vec![2, 3], vec![2, 3]), "then dependency and build folders");
    assert_eq!(held(3), (vec![2, 3], vec![0, 1, 2, 3]), "then everything on other volumes");
    assert_eq!(held(4), (vec![1, 2, 3], vec![0, 1, 2, 3]), "then the boot volume's system data");
    assert_eq!(held(5), held(4), "the hard stop evicts nothing more, and user files are never held back");
}

#[test]
fn build_storms_are_noisy() {
    let plan = plan(true);
    for path in [
        "Projects/app/node_modules/x/index.js",
        "Projects/app/target/debug/app.d",
        "Projects/app/.git/index",
        "Library/Caches/com.app/data",
        ".cargo/registry/a.crate",
        ".Trash/old.txt",
    ] {
        assert!(plan.noisy(&home(path)), "{path}");
    }
    for path in [
        "Projects/app/src/main.rs",
        "Documents/report.pdf",
        "Developer/app/main.swift",
        "go.mod",
        "Library/Preferences/a.plist",
    ] {
        assert!(!plan.noisy(&home(path)), "{path}");
    }
}

#[test]
fn folder_uses_its_roots_devices() {
    let plan = plan(true);
    assert_eq!(plan.devices(&home("Music")), [1, 1]);
    assert_eq!(plan.devices(Path::new("/disk/Volumes/Work/a")), [2, 2], "the longest root wins");
    assert_eq!(plan.devices(Path::new("/nowhere")), [0, 0]);
}

#[test]
fn tier_follows_from_path() {
    let plan = plan(true);
    let inside = |below: &str| plan.inside(&home(below));
    assert_eq!((inside(""), inside("Projects/app/src"), inside("Developer/app")), (0, 0, 0));
    assert_eq!(
        (inside("Library"), inside("Library/Application Support/App"), inside("Applications/Tool.app/Contents")),
        (1, 1, 1)
    );
    assert_eq!(
        (inside(".cargo/registry"), inside("Projects/app/node_modules/x"), inside("Projects/app/.git")),
        (2, 2, 2)
    );
    assert_eq!((inside("Library/Caches/x"), inside(".Trash"), inside("Projects/app/node_modules/x/.cache")), (3, 3, 3));
    assert_eq!(plan.inside(Path::new("/disk/Volumes/Work/Library")), 0, "only the home Library is system data");
    assert!(plan.packaged(&home("Applications/Tool.app/Contents")) && plan.packaged(&home("Tool.app")));
    assert!(!plan.packaged(&home("Projects/app")));
}

#[test]
fn apps_rank_before_home() {
    let apps = ["/Applications", "/System/Applications", "/Users/me/Applications"].map(PathBuf::from).to_vec();
    let planned =
        |fences| Plan::fenced(&FindConfig::default(), &[boot("/")], Some(Path::new("/Users/me")), None, true, fences);
    let plan = planned(Fences { apps, ..Fences::default() });
    let rank = |path: &str| plan.rank(Path::new(path));
    assert_eq!((rank("/Applications/Safari.app"), rank("/System/Applications"), rank("/Applications")), (0, 0, 0));
    assert_eq!((rank("/System"), rank("/Users/me"), rank("/Users"), rank("/")), (0, 0, 0, 0), "the way there too");
    assert_eq!(rank("/Users/me/Applications"), 0, "the apps at home lead like the others");
    assert_eq!((rank("/Users/me/Music"), rank("/Users/me/Documents/GitHub")), (1, 1), "then the rest of home");
    assert_eq!((rank("/Library"), rank("/Users/other"), rank("/opt/homebrew")), (2, 2, 2));
    let bare = planned(Fences::default());
    assert_eq!(bare.rank(Path::new("/Applications")), 2, "a plan that was told of no apps ranks none");
    assert!(!bare.leads_to_apps(Path::new("/")) && !bare.ahead(Path::new("/Applications")));
    assert!(!Plan::resting().leads_to_apps(Path::new("/")));
}

#[test]
fn native_plan_follows_platform() {
    let home = Path::new("/Users/me");
    let limited = Plan::new(&FindConfig::default(), &[boot("/")], Some(home), None, false);
    let sealed = crate::find::os::sealed();
    assert!(sealed.iter().all(|wall| !limited.wanted(&wall.join("inside"))));
    assert!(crate::find::os::guarded(home).iter().all(|folder| !limited.wanted(&folder.join("a.txt"))));
    assert!(limited.wanted(&home.join("Music/a.mp3")));
}
