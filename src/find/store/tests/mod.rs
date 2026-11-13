mod refused;

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::find::model::{Entry, EntryId, Live, Room, flag, with_tier};
use crate::find::tests::Scratch;
use crate::find::tests::corpus::grown;

use super::{INDEX_FILE, Meta, RootMark, USAGE_FILE, dir, load, mark, meta, save};

const ROOMY: Room = Room { cap: usize::MAX, level: 0, boot: None };
const UNFINISHED: &str = "index.bin.tmp";
const ODD_NAMES: [&str; 6] = [
    "R\u{e9}sum\u{e9} FINAL.PDF",
    "Re\u{301}sume\u{301}.pdf",
    "\u{41e}\u{442}\u{447}\u{451}\u{442}.docx",
    "\u{6587}\u{4ef6} Copy.TXT",
    "MAKEFILE",
    "makefile",
];

fn settled(mut live: Live) -> Live {
    let rebuilt = live.rebuilt(&ROOMY);
    live.adopt(rebuilt);
    live
}

fn sample(seed: u64) -> Live {
    let mut live = grown(seed, 2_000);
    let folders: Vec<EntryId> = (0..live.len() as EntryId).filter(|id| live.flags(*id) & flag::DIR != 0).collect();
    for (nth, folder) in folders.iter().enumerate().filter(|(nth, _)| nth % 9 == 0) {
        let flags = with_tier(flag::HIDDEN | flag::CLOUD, (nth % 4) as u8);
        for name in ODD_NAMES {
            live.add(Some(*folder), Entry { name, modified: 1_700_000_000 + nth as u32, created: nth as u32, flags });
        }
    }
    live.add(None, Entry { name: "/Volumes/\u{c9}t\u{e9}", modified: 0, created: 0, flags: flag::DIR });
    settled(live)
}

fn marks() -> Meta {
    let root = |path: &str, volume: u8, flags: u8, scanned_at: u32, swept_at: u32| RootMark {
        path: PathBuf::from(path),
        volume: [volume; 16],
        flags,
        scanned_at,
        swept_at,
    };
    Meta {
        saved_at: 1_759_700_000,
        cursor: 0xfeed_0000_1234_5678,
        level: 2,
        flags: meta::GUARDED_DONE,
        roots: vec![
            root("/", 0xa1, mark::COMPLETE, 1_759_600_000, 1_759_000_000),
            root("/Volumes/\u{c9}t\u{e9}", 0xb2, 0, 0, 0),
            root("/Volumes/Work", 0xc3, mark::COMPLETE, 1_759_650_000, 0),
        ],
    }
}

#[test]
fn index_lives_in_local_data() {
    assert_eq!(dir(), crate::platform::local_data_dir().map(|data| data.join("find")));
    assert_ne!(INDEX_FILE, USAGE_FILE);
}

#[test]
fn saved_index_loads_back_equal() {
    let scratch = Scratch::new("store-round-trip");
    let live = sample(31);
    save(scratch.dir(), &live, &marks()).unwrap();
    let (loaded, meta) = load(scratch.dir()).expect("the snapshot we just wrote");
    assert!(loaded.base() == live.base(), "every column, name and flag");
    assert_eq!(meta, marks());
    assert_eq!((loaded.len(), loaded.alive(), loaded.generation()), (live.len(), live.len(), 0));
    assert!(loaded.settled() && scratch.file(INDEX_FILE).exists() && !scratch.file(UNFINISHED).exists());
    let (mut path, mut name) = (PathBuf::new(), String::new());
    for id in 0..live.len() as EntryId {
        live.path_into(id, &mut path);
        assert_eq!(loaded.resolve(&path), Some(id), "{}", path.display());
        loaded.name_into(id, &mut name);
        assert!(path.ends_with(&name) && loaded.key(id) == live.key(id), "{}", path.display());
    }
}

#[test]
fn loaded_index_keeps_working() {
    let scratch = Scratch::new("store-reuse");
    save(scratch.dir(), &sample(32), &Meta::default()).unwrap();
    let (mut loaded, _) = load(scratch.dir()).unwrap();
    let rows = loaded.len();
    let late = loaded.add(Some(0), Entry { name: "Late.txt", modified: 5, created: 5, flags: 0 });
    loaded.bury(1);
    loaded.settle();
    assert_eq!(loaded.resolve(Path::new("/Late.txt")), Some(late));
    let loaded = settled(loaded);
    assert!(loaded.len() < rows + 1 && loaded.resolve(Path::new("/Late.txt")).is_some());
    save(scratch.dir(), &loaded, &Meta::default()).unwrap();
    assert!(load(scratch.dir()).is_some_and(|(again, _)| again.base() == loaded.base()));
}

#[test]
fn empty_index_saves_too() {
    let scratch = Scratch::new("store-empty");
    save(scratch.dir(), &Live::empty(), &Meta::default()).unwrap();
    let (loaded, meta) = load(scratch.dir()).expect("an empty snapshot");
    assert_eq!((loaded.len(), meta), (0, Meta::default()));
    assert_eq!(loaded.arenas().map(|arena| arena.bytes.to_vec()), [vec![0], vec![0]]);
}

#[test]
fn snapshot_keeps_root_marks() {
    let scratch = Scratch::new("store-marks");
    save(scratch.dir(), &sample(33), &marks()).unwrap();
    let (_, meta) = load(scratch.dir()).unwrap();
    assert_eq!(meta.roots.iter().map(|root| root.flags & mark::COMPLETE).collect::<Vec<u8>>(), [1, 0, 1]);
    assert_eq!(meta.roots[0].scanned_at, 1_759_600_000);
    assert_eq!((meta.roots[0].swept_at, meta.roots[2].swept_at), (1_759_000_000, 0));
    assert_eq!(meta.roots[1].path, Path::new("/Volumes/\u{c9}t\u{e9}"));
    assert_eq!(meta.roots.iter().map(|root| root.volume[15]).collect::<Vec<u8>>(), [0xa1, 0xb2, 0xc3]);
    assert_eq!((meta.cursor, meta.level, meta.flags, meta.saved_at), (0xfeed_0000_1234_5678, 2, 1, 1_759_700_000));
}

#[test]
fn only_settled_index_is_saved() {
    let scratch = Scratch::new("store-unsettled");
    let mut live = sample(34);
    live.bury(5);
    assert_eq!(save(scratch.dir(), &live, &Meta::default()).unwrap_err().kind(), io::ErrorKind::InvalidInput);
    let mut live = settled(live);
    live.add(Some(0), Entry { name: "new", modified: 0, created: 0, flags: 0 });
    assert_eq!(save(scratch.dir(), &live, &Meta::default()).unwrap_err().kind(), io::ErrorKind::InvalidInput);
    assert!(!scratch.file(INDEX_FILE).exists() && load(scratch.dir()).is_none());
}

#[test]
fn bad_marks_refused_before_writing() {
    let scratch = Scratch::new("store-too-much");
    let endless = RootMark { path: "/x".repeat(40_000).into(), ..marks().roots.remove(0) };
    let crowded = Meta { roots: vec![marks().roots.remove(0); usize::from(u16::MAX) + 1], ..Meta::default() };
    for unwritable in [Meta { roots: vec![endless], ..Meta::default() }, crowded] {
        let refused = save(scratch.dir(), &sample(35), &unwritable).unwrap_err();
        assert_eq!(refused.kind(), io::ErrorKind::InvalidInput);
    }
    assert!(!scratch.file(INDEX_FILE).exists());
}

#[test]
fn interrupted_write_keeps_snapshot() {
    let scratch = Scratch::new("store-interrupted");
    let (older, newer) = (sample(36), sample(37));
    save(scratch.dir(), &older, &marks()).unwrap();
    fs::write(scratch.file(UNFINISHED), b"half a snapsh").unwrap();
    let (loaded, meta) = load(scratch.dir()).expect("the snapshot from before the crash");
    assert!(loaded.base() == older.base() && meta == marks());
    save(scratch.dir(), &newer, &Meta::default()).unwrap();
    assert!(load(scratch.dir()).is_some_and(|(loaded, _)| loaded.base() == newer.base()));
    assert!(!scratch.file(UNFINISHED).exists());
}
