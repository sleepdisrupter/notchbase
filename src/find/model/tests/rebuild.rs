use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use crate::find::key::Key;
use crate::find::model::base::rank;
use crate::find::model::{Entry, EntryId, Live, Room, flag, with_tier};
use crate::find::tests::corpus::{Planted, from_paths, grown, plant};

const ROOMY: Room = Room { cap: usize::MAX, level: 0, boot: None };
const ARCHIVE: &str = "/Volumes/Archive";
const TIERED: [(&str, u8); 11] = [
    ("/Users/john/notes.txt", 0),
    ("/Users/john/Library/", 0),
    ("/Users/john/Library/Preferences/", 1),
    ("/Users/john/Library/Preferences/app.plist", 1),
    ("/Users/john/Library/Caches/", 1),
    ("/Users/john/Library/Caches/blob.bin", 3),
    ("/Users/john/code/node_modules/", 0),
    ("/Users/john/code/node_modules/left-pad/", 2),
    ("/Users/john/code/node_modules/left-pad/index.js", 2),
    ("/Users/john/code/main.rs", 0),
    ("/Applications/Safari.app/", 0),
];

fn rebuild(live: &mut Live, room: &Room) {
    let rebuilt = live.rebuilt(room);
    live.adopt(rebuilt);
}

fn path_of(live: &Live, id: EntryId) -> PathBuf {
    let mut path = PathBuf::new();
    live.path_into(id, &mut path);
    path
}

fn alive(live: &Live) -> Vec<EntryId> {
    (0..live.len() as EntryId).filter(|id| !live.is_dead(*id)).collect()
}

fn churned(seed: u64) -> Live {
    let mut live = grown(seed, 3_000);
    rebuild(&mut live, &ROOMY);
    let folders: Vec<EntryId> = alive(&live).into_iter().filter(|id| live.flags(*id) & flag::DIR != 0).collect();
    for (nth, folder) in folders.iter().enumerate().filter(|(nth, _)| nth % 3 == 0) {
        let name = format!("Added-{nth}.TXT");
        let twin = name.to_lowercase();
        let inside = live.add(Some(*folder), Entry { name: "zz-new", modified: 4, created: 4, flags: flag::DIR });
        for name in [name.as_str(), twin.as_str(), "\u{c9}t\u{e9}.md", "ete.md"] {
            live.add(Some(inside), Entry { name, modified: nth as u32, created: 2, flags: flag::CLOUD });
            live.add(Some(*folder), Entry { name, modified: nth as u32, created: 2, flags: 0 });
        }
    }
    alive(&live).into_iter().filter(|id| id % 11 == 5).for_each(|id| live.bury(id));
    live.settle();
    live
}

fn tiered() -> (Live, EntryId, EntryId) {
    let mut live = Live::empty();
    for (path, tier) in TIERED {
        plant(&mut live, &Planted { path, modified: 9, created: 9, flags: with_tier(0, tier) });
    }
    let archive = live.add(None, Entry { name: ARCHIVE, modified: 0, created: 0, flags: with_tier(flag::DIR, 3) });
    let library = live.add(Some(archive), Entry { name: "Library", modified: 1, created: 1, flags: flag::DIR });
    live.add(Some(library), Entry { name: "old.plist", modified: 1, created: 1, flags: with_tier(0, 1) });
    live.add(Some(archive), Entry { name: "old.txt", modified: 1, created: 1, flags: 0 });
    let boot = live.resolve(Path::new("/")).expect("the boot volume");
    (live, boot, archive)
}

fn kept(level: u8, names_the_boot_volume: bool) -> Vec<String> {
    let (mut live, boot, _) = tiered();
    rebuild(&mut live, &Room { cap: usize::MAX, level, boot: names_the_boot_volume.then_some(boot) });
    assert!(live.settled());
    let mut paths: Vec<String> = alive(&live).iter().map(|id| path_of(&live, *id).display().to_string()).collect();
    paths.sort();
    paths
}

fn gone_since(before: &[String], after: &[String]) -> Vec<String> {
    assert!(after.iter().all(|path| before.contains(path)), "a higher level never brings anything back");
    before.iter().filter(|path| !after.contains(path)).cloned().collect()
}

#[test]
fn rebuild_keeps_order() {
    for seed in [1, 2, 3] {
        let mut live = churned(seed);
        let survivors = live.alive();
        rebuild(&mut live, &ROOMY);
        assert!(live.settled() && live.len() == survivors);
        let columns = &live.base().columns;
        assert_eq!(live.parent(0), None, "a root comes first");
        for row in 1..columns.len() {
            let (above, parent) = (columns.parent[row - 1], columns.parent[row]);
            assert!(rank(parent) as usize <= row, "row {row} comes after its parent");
            assert!(rank(above) <= rank(parent), "the parent column never steps back at row {row}");
            assert!(above != parent || columns.order(row - 1, columns, row) == Ordering::Less, "siblings at {row}");
        }
        let mut children = Vec::new();
        for id in 0..live.len() as EntryId {
            live.children_into(id, &mut children);
            assert!(children.windows(2).all(|pair| pair[0] + 1 == pair[1]), "the children of {id} are one run");
            assert!(children.iter().all(|child| live.parent(*child) == Some(id)));
        }
    }
}

#[test]
fn path_resolves_across_rebuild() {
    let mut live = churned(4);
    let describe = |live: &Live, id: EntryId| -> (PathBuf, Key, u32, u32, u8) {
        (path_of(live, id), live.key(id), live.modified(id), live.created(id), live.flags(id))
    };
    let before: Vec<_> = alive(&live).into_iter().map(|id| describe(&live, id)).collect();
    assert!(before.iter().enumerate().all(|(nth, row)| describe(&live, live.resolve(&row.0).unwrap()) == before[nth]));
    for round in 0..2 {
        rebuild(&mut live, &ROOMY);
        assert_eq!((live.len(), live.generation()), (before.len(), round + 2));
        let mut seen = vec![false; live.len()];
        for row in &before {
            let id = live.resolve(&row.0).unwrap_or_else(|| panic!("{} survived", row.0.display()));
            assert_eq!(&describe(&live, id), row);
            assert!(!std::mem::replace(&mut seen[id as usize], true), "{} has an entry of its own", row.0.display());
        }
    }
}

#[test]
fn unsettled_burial_drops_whole() {
    let mut live = from_paths(&["/gone/deep/er/b.txt", "/gone/c.txt", "/kept/a.txt"]);
    rebuild(&mut live, &ROOMY);
    let gone = live.resolve(Path::new("/gone")).unwrap();
    live.add(Some(gone), Entry { name: "late.txt", modified: 1, created: 1, flags: 0 });
    live.bury(gone);
    rebuild(&mut live, &ROOMY);
    let left: Vec<PathBuf> = alive(&live).into_iter().map(|id| path_of(&live, id)).collect();
    assert_eq!(left, [Path::new("/"), Path::new("/kept"), Path::new("/kept/a.txt")]);
}

#[test]
fn valve_drops_in_order() {
    let levels: Vec<Vec<String>> = (0..=5).map(|level| kept(level, true)).collect();
    assert_eq!(levels[0].len(), tiered().0.len(), "level 0 keeps everything");
    assert_eq!(gone_since(&levels[0], &levels[1]), ["/Users/john/Library/Caches/blob.bin"]);
    assert_eq!(
        gone_since(&levels[1], &levels[2]),
        ["/Users/john/code/node_modules/left-pad", "/Users/john/code/node_modules/left-pad/index.js"]
    );
    assert_eq!(
        gone_since(&levels[2], &levels[3]),
        ["/Volumes/Archive/Library", "/Volumes/Archive/Library/old.plist", "/Volumes/Archive/old.txt"]
    );
    assert_eq!(
        gone_since(&levels[3], &levels[4]),
        ["/Users/john/Library/Caches", "/Users/john/Library/Preferences", "/Users/john/Library/Preferences/app.plist"]
    );
    assert_eq!(levels[4], levels[5], "the hard stop evicts nothing more");
    let user_files = ["/Users/john/notes.txt", "/Users/john/code/main.rs", "/Applications/Safari.app"];
    let containers = ["/Users/john/Library", "/Users/john/code/node_modules", ARCHIVE, "/"];
    for path in user_files.iter().chain(&containers) {
        assert!(levels.iter().all(|kept| kept.iter().any(|found| found == path)), "{path} is never evicted");
    }
}

#[test]
fn no_boot_volume_keeps_user_files() {
    let (named, unnamed) = (kept(5, true), kept(5, false));
    assert_eq!(gone_since(&unnamed, &named), ["/Volumes/Archive/Library", "/Volumes/Archive/old.txt"]);
    assert!(!unnamed.iter().any(|path| path.ends_with("old.plist")), "its library still goes");
}

#[test]
fn rebuild_evicts_only_its_level() {
    let (mut live, boot, _) = tiered();
    let rows = live.len();
    rebuild(&mut live, &Room { cap: 1, level: 0, boot: Some(boot) });
    assert_eq!((live.len(), live.alive()), (rows, rows));
}
