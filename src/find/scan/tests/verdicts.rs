use std::path::{Path, PathBuf};

use crate::config::FindConfig;
use crate::find::os::{Root, raw};

use super::super::Verdict::{self, Descend, Leaf, Skip};
use super::super::{Fences, MAX_DEPTH, Plan};
use super::{DIR, boot, removable};

const HIDDEN: u8 = DIR | raw::HIDDEN;
const LINK: u8 = raw::LINK;
const FILE: u8 = 0;
const SHALLOW: usize = 3;
pub(super) const EVERYONES_MENU: &str = "ProgramData\\Microsoft\\Windows\\Start Menu\\Programs";
pub(super) const OWN_MENU: &str = "Users\\me\\AppData\\Roaming\\Microsoft\\Windows\\Start Menu\\Programs";

type Case<'a> = (&'a str, u8, &'a str, u8, Verdict);

pub(super) fn mac(full_access: bool) -> Plan {
    let home = Path::new("/Users/me");
    let all = |paths: &[&str]| paths.iter().map(PathBuf::from).collect();
    let fences = Fences {
        sealed: all(&["/System", "/usr", "/bin", "/sbin", "/dev", "/private/var/db", "/private/var/vm", "/Volumes"]),
        open: all(&["/System/Applications", "/usr/local"]),
        guarded: ["Desktop", "Documents", "Downloads", "Library/Mobile Documents", "Library/Containers"]
            .map(|folder| home.join(folder))
            .to_vec(),
        apps: all(&["/Applications", "/System/Applications", "/Users/me/Applications"]),
    };
    let roots = [boot("/"), removable("/Volumes/Work")];
    Plan::fenced(&FindConfig::default(), &roots, Some(home), None, full_access, fences)
}

pub(super) fn drive(letter: char, below: &str) -> PathBuf {
    below
        .split('\\')
        .filter(|part| !part.is_empty())
        .fold(PathBuf::from(format!("{letter}:\\")), |path, part| path.join(part))
}

pub(super) fn windows() -> Plan {
    let fences = Fences {
        apps: vec![drive('C', EVERYONES_MENU), drive('C', OWN_MENU)],
        sealed: vec![
            drive('C', "Windows"),
            drive('C', "$Recycle.Bin"),
            drive('C', "System Volume Information"),
            drive('D', "$Recycle.Bin"),
            drive('D', "System Volume Information"),
        ],
        ..Fences::default()
    };
    let second = Root { path: drive('D', ""), volume: [0; 16], devices: [0, 0], flags: 0 };
    let roots = [Root { devices: [0, 0], ..boot("C:\\") }, second];
    Plan::fenced(&FindConfig::default(), &roots, Some(&drive('C', "Users\\me")), None, true, fences)
}

fn check(plan: &Plan, cases: &[Case<'_>], parent: impl Fn(&str) -> PathBuf) {
    for (below, tier, name, bits, expected) in cases {
        let verdict = plan.verdict(&parent(below), *tier, name, *bits, SHALLOW);
        assert_eq!(verdict, *expected, "{name} in {below} at tier {tier}");
    }
}

#[test]
fn mac_boot_volume_layout() {
    let cases: &[Case<'_>] = &[
        ("/", 0, "Users", DIR, Descend(0)),
        ("/", 0, "Applications", DIR, Descend(0)),
        ("/", 0, "Library", DIR, Descend(1)),
        ("/", 0, "opt", DIR, Descend(1)),
        ("/", 0, "private", DIR, Descend(1)),
        ("/private", 1, "tmp", DIR, Descend(3)),
        ("/private/var", 1, "folders", DIR, Descend(3)),
        ("/private/var", 1, "db", DIR, Leaf),
        ("/", 0, "System", DIR, Descend(0)),
        ("/System", 0, "Applications", DIR, Descend(0)),
        ("/System", 0, "Library", DIR, Skip),
        ("/System", 0, "a-file", FILE, Skip),
        ("/System/Applications", 0, "Maps.app", DIR, Descend(1)),
        ("/", 0, "usr", DIR, Descend(1)),
        ("/usr", 1, "local", DIR, Descend(1)),
        ("/usr", 1, "bin", DIR, Skip),
        ("/", 0, "bin", DIR, Leaf),
        ("/", 0, "dev", DIR, Leaf),
        ("/", 0, "Volumes", DIR, Leaf),
        ("/", 0, "tmp", LINK, Leaf),
        ("/", 0, "etc", DIR | LINK, Leaf),
        ("/", 0, ".file", FILE, Leaf),
        ("/Volumes/Work", 0, "System", DIR, Descend(0)),
        ("/Volumes/Work", 0, "Library", DIR, Descend(0)),
    ];
    check(&mac(true), cases, |parent| PathBuf::from(parent));
}

#[test]
fn home_is_user_content() {
    let cases: &[Case<'_>] = &[
        ("/Users/me", 0, "Music", DIR, Descend(0)),
        ("/Users/me", 0, "notes.txt", FILE, Leaf),
        ("/Users/me", 0, "Library", DIR, Descend(1)),
        ("/Users/me/Library", 1, "Caches", DIR, Descend(3)),
        ("/Users/me/Library", 1, "Application Support", DIR, Descend(1)),
        ("/Users/me", 0, ".Trash", HIDDEN, Descend(3)),
        ("/Users/me", 0, ".cargo", HIDDEN, Descend(2)),
        ("/Users/me/.cargo", 2, "registry", DIR, Descend(2)),
        ("/Users/me", 0, "go", DIR, Descend(2)),
        ("/Users/me", 0, "Developer", DIR, Descend(0)),
        ("/Users/me/Projects", 0, "go", DIR, Descend(0)),
        ("/Users/me/Projects/app", 0, "node_modules", DIR, Descend(2)),
        ("/Users/me/Projects/app", 0, ".git", HIDDEN, Descend(2)),
        ("/Users/me/Projects/app", 0, "DerivedData", DIR, Descend(2)),
        ("/Users/me/Projects/app/node_modules", 2, "Cache", DIR, Descend(3)),
        ("/Users/me/Documents", 0, "Cache", DIR, Descend(0)),
        ("/Users/me/Documents", 0, "temp", DIR, Descend(0)),
        ("/Users/me/Music", 0, "Library", DIR, Descend(0)),
        ("/Applications", 0, "Xcode.app", DIR, Descend(1)),
        ("/Applications/Xcode.app", 1, "Contents", DIR, Descend(1)),
        ("/Users/me/Movies", 0, "Cut.fcpbundle", DIR, Descend(1)),
        ("/Users/me/Pictures", 0, "Photos Library.photoslibrary", DIR, Descend(1)),
    ];
    check(&mac(true), cases, |parent| PathBuf::from(parent));
}

#[test]
fn leaves_are_still_recorded() {
    let plan = mac(true);
    let home = Path::new("/Users/me");
    let cloud = DIR | raw::DATALESS | raw::CLOUD;
    assert_eq!(plan.verdict(home, 0, "iCloud Folder", cloud, SHALLOW), Leaf);
    assert_eq!(plan.verdict(home, 0, "mounted", DIR | raw::ELSEWHERE, SHALLOW), Leaf);
    assert_eq!(plan.verdict(home, 0, "to-folder", LINK, SHALLOW), Leaf);
    assert_eq!(plan.verdict(home, 0, "deep", DIR, MAX_DEPTH), Leaf);
    assert_eq!(plan.verdict(home, 0, "deep", DIR, MAX_DEPTH - 1), Descend(0));
    let backup = plan.verdict(Path::new("/Volumes/Work"), 0, "2026-10-06-101500.previous", DIR, SHALLOW);
    assert_eq!(backup, Leaf, "the newest backup on a backup disk is one row");
    assert_eq!(plan.verdict(home, 0, "notes.previous", DIR, SHALLOW), Descend(0));
}

#[test]
fn guarded_open_once_full() {
    let cases = [
        ("/Users/me", 0, "Documents", Descend(0)),
        ("/Users/me", 0, "Desktop", Descend(0)),
        ("/Users/me", 0, "Downloads", Descend(0)),
        ("/Users/me/Library", 1, "Mobile Documents", Descend(1)),
        ("/Users/me/Library", 1, "Containers", Descend(1)),
        ("/Users/me/Pictures", 0, "Photos Library.photoslibrary", Descend(1)),
        ("/Users/me/Music/Music", 0, "Music Library.musiclibrary", Descend(1)),
        ("/Users/me/Movies/TV", 0, "TV Library.tvlibrary", Descend(1)),
    ];
    let (limited, full) = (mac(false), mac(true));
    for (parent, tier, name, opened) in cases {
        assert_eq!(limited.verdict(Path::new(parent), tier, name, DIR, SHALLOW), Leaf, "{name} while limited");
        assert_eq!(full.verdict(Path::new(parent), tier, name, DIR, SHALLOW), opened, "{name} with full access");
    }
    assert_eq!(limited.verdict(Path::new("/Users/me"), 0, "Music", DIR, SHALLOW), Descend(0));
    assert_eq!(limited.verdict(Path::new("/Users/me"), 0, "Xcode.app", DIR, SHALLOW), Descend(1));
}

#[test]
fn windows_drive_layout() {
    let cases: &[Case<'_>] = &[
        ("C:", 0, "Users", DIR, Descend(0)),
        ("C:", 0, "Projects", DIR, Descend(0)),
        ("C:", 0, "Windows", DIR, Leaf),
        ("C:", 0, "$Recycle.Bin", HIDDEN, Leaf),
        ("C:", 0, "System Volume Information", HIDDEN, Leaf),
        ("C:", 0, "Program Files", DIR, Descend(1)),
        ("C:", 0, "Program Files (x86)", DIR, Descend(1)),
        ("C:", 0, "ProgramData", HIDDEN, Descend(1)),
        ("C:", 0, "pagefile.sys", raw::HIDDEN, Leaf),
        ("C:Users\\me", 0, "AppData", HIDDEN, Descend(1)),
        ("C:Users\\me\\AppData\\Local", 1, "Temp", DIR, Descend(3)),
        ("C:Users\\me", 0, "Documents", DIR, Descend(0)),
        ("C:Users\\me", 0, "Application Data", LINK, Leaf),
        ("C:Users\\me\\source\\app", 0, "node_modules", DIR, Descend(2)),
        ("D:", 0, "Windows", DIR, Descend(0)),
        ("D:", 0, "$Recycle.Bin", HIDDEN, Leaf),
    ];
    let parent = |below: &str| drive(below.chars().next().unwrap_or('C'), below.get(2..).unwrap_or(""));
    check(&windows(), cases, parent);
    let plan = windows();
    assert!(
        !plan.wanted(&drive('C', "Windows\\System32\\drivers")) && !plan.wanted(&drive('D', "$Recycle.Bin\\S-1-5"))
    );
    assert!(plan.wanted(&drive('D', "Windows\\notes.txt")) && plan.wanted(&drive('C', "Users\\me\\Documents\\a.docx")));
    assert!(!plan.wanted(&drive('E', "usb\\file.txt")), "a drive that is not a root is not scanned");
    assert!(plan.held_back(3, &drive('D', "Archive"), 0) && !plan.held_back(3, &drive('C', "Users"), 0));
    assert_eq!(plan.inside(&drive('C', "Users\\me\\AppData\\Local\\Temp\\x")), 3);
}
