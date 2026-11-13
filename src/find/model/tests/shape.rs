use std::path::Path;

use crate::find::model::{Entry, EntryId, Live, MERGE_ROWS, OVERLAY_MAX, Room, flag, fold_query, overlay_limit};
use crate::find::tests::corpus::{from_paths, grown};

const ROOMY: Room = Room { cap: usize::MAX, level: 0, boot: None };
const FIRST_SCAN: usize = 1_450_000;
const ALIKE: [&str; 4] = ["Re\u{301}sume\u{301}.pdf", "R\u{e9}sum\u{e9}.pdf", "resume.pdf", "r\u{e9}sum\u{e9}.pdf"];
const PLAIN: usize = 2;

fn rebuild(live: &mut Live) {
    let rebuilt = live.rebuilt(&ROOMY);
    live.adopt(rebuilt);
}

fn file(name: &str) -> Entry<'_> {
    Entry { name, modified: 10, created: 5, flags: 0 }
}

fn found(live: &Live, path: &str) -> EntryId {
    live.resolve(Path::new(path)).unwrap_or_else(|| panic!("{path} is in the index"))
}

fn children(live: &Live, path: &str) -> Vec<String> {
    let (mut ids, mut name) = (Vec::new(), String::new());
    live.children_into(found(live, path), &mut ids);
    let named = |id: &EntryId| {
        live.name_into(*id, &mut name);
        name.clone()
    };
    ids.iter().map(named).collect()
}

#[test]
fn deleting_folder_hides_below() {
    let mut live = from_paths(&["/keep/a.txt", "/gone/deep/er/b.txt", "/gone/c.txt"]);
    rebuild(&mut live);
    let (gone, deep) = (found(&live, "/gone"), found(&live, "/gone/deep"));
    let newer = live.add(Some(deep), Entry { flags: flag::DIR, ..file("newer") });
    let newest = live.add(Some(newer), file("d.txt"));
    let before = live.alive();
    live.bury(gone);
    assert!(!live.is_dead(newest), "nothing below it is hidden before the batch settles");
    live.settle();
    assert_eq!(live.alive(), before - 7);
    for path in ["/gone", "/gone/c.txt", "/gone/deep", "/gone/deep/er/b.txt", "/gone/deep/newer/d.txt"] {
        assert_eq!(live.resolve(Path::new(path)), None, "{path}");
    }
    assert!(live.is_dead(newest) && live.is_dead(deep));
    assert_eq!((children(&live, "/"), children(&live, "/keep")), (vec!["keep".to_owned()], vec!["a.txt".to_owned()]));
    let late = live.add(Some(deep), file("too-late.txt"));
    live.settle();
    assert!(live.is_dead(late), "a row added under a deleted folder is hidden by the next settle");
    assert_eq!(live.alive(), before - 7);
}

#[test]
fn fold_twins_stay_two_entries() {
    let mut live = from_paths(&["/d/resume.pdf", "/d/other.txt"]);
    rebuild(&mut live);
    let dir = found(&live, "/d");
    for name in ALIKE.iter().rev().filter(|name| **name != ALIKE[PLAIN]) {
        live.add(Some(dir), file(name));
    }
    for round in 0..3 {
        let ids: Vec<EntryId> = ALIKE.iter().map(|name| found(&live, &format!("/d/{name}"))).collect();
        assert!(ids.iter().all(|id| live.folded(*id) == fold_query("resume.pdf")), "round {round}");
        assert!((1..ids.len()).all(|nth| !ids[..nth].contains(&ids[nth])), "four entries in round {round}");
        let accented = (0..ids.len()).filter(|nth| *nth != PLAIN);
        assert!(accented.clone().all(|nth| live.key(ids[nth]) != live.key(ids[PLAIN])), "an accent is part of a key");
        assert_ne!(live.key(ids[0]), live.key(ids[1]), "and so is the way it is spelled");
        assert_eq!(children(&live, "/d"), ["other.txt", ALIKE[0], ALIKE[1], ALIKE[2], ALIKE[3]], "round {round}");
        live.touch(ids[0], 77 + round);
        assert_eq!(ids.iter().map(|id| live.modified(*id)).filter(|stamp| *stamp == 77 + round).count(), 1);
        rebuild(&mut live);
    }
    live.bury(found(&live, "/d/resume.pdf"));
    assert_eq!(live.resolve(Path::new("/d/resume.pdf")), None);
    assert_eq!(children(&live, "/d"), ["other.txt", ALIKE[0], ALIKE[1], ALIKE[3]], "its twins never moved");
    let back = live.add(Some(found(&live, "/d")), file("resume.pdf"));
    assert_eq!(found(&live, "/d/resume.pdf"), back);
}

#[test]
fn overlay_threshold_doubles() {
    assert_eq!((overlay_limit(0), overlay_limit(1_000), overlay_limit(MERGE_ROWS)), (65_536, 65_536, 65_536));
    assert_eq!((overlay_limit(131_072), overlay_limit(524_288)), (131_072, 524_288));
    assert_eq!((overlay_limit(OVERLAY_MAX), overlay_limit(6_700_000)), (1_048_576, 1_048_576));
    let mut merges = Vec::new();
    let mut base = 0;
    while base + overlay_limit(base) <= FIRST_SCAN {
        base += overlay_limit(base);
        merges.push(base);
    }
    assert_eq!(merges, [65_536, 131_072, 262_144, 524_288, 1_048_576], "five on the way, a sixth at the end");
}

#[test]
fn dead_rows_force_rebuild() {
    let mut live = grown(6, 800);
    assert!(!live.wants_rebuild(), "eight hundred rows are far from a merge");
    rebuild(&mut live);
    (1..=100).for_each(|id| live.bury(id));
    assert!(!live.wants_rebuild(), "exactly an eighth dead is not yet past it");
    live.bury(101);
    assert!(live.wants_rebuild());
    rebuild(&mut live);
    assert!(!live.wants_rebuild() && live.settled());
}

#[test]
fn settled_and_added_read_as_one() {
    let mut live = from_paths(&["/d/a.txt", "/d/c.txt", "/d/sub/x"]);
    rebuild(&mut live);
    let (dir, settled) = (found(&live, "/d"), live.len());
    let added: Vec<EntryId> = ["b.txt", "A.txt", "d.txt"].iter().map(|name| live.add(Some(dir), file(name))).collect();
    assert_eq!(added, [settled as EntryId, settled as EntryId + 1, settled as EntryId + 2]);
    assert_eq!(children(&live, "/d"), ["A.txt", "a.txt", "b.txt", "c.txt", "d.txt", "sub"]);
    let [base, overlay] = live.arenas();
    assert_eq!((base.first, overlay.first as usize), (0, settled));
    assert_eq!(overlay.bytes, b"\0b.txt\0a.txt\0d.txt\0");
    assert_eq!((live.id_at(1, 1), live.id_at(1, 8), live.id_at(1, 17)), (added[0], added[1], added[2]));
    assert_eq!(live.id_at(0, base.bytes.len() - 2) as usize, settled - 1);
    live.bury(found(&live, "/d/a.txt"));
    live.bury(added[0]);
    assert_eq!(children(&live, "/d"), ["A.txt", "c.txt", "d.txt", "sub"]);
    assert_eq!((live.len(), live.alive()), (settled + 3, settled + 1));
}

#[test]
fn path_starts_at_longest_root() {
    let mut live = from_paths(&["/Volumes/Work"]);
    rebuild(&mut live);
    let work = live.add(None, Entry { flags: flag::DIR, ..file("/Volumes/Work") });
    let notes = live.add(Some(work), file("notes.txt"));
    assert_eq!(live.resolve(Path::new("/Volumes/Work/notes.txt")), Some(notes));
    assert_eq!(
        live.resolve(Path::new("/Volumes/Work")),
        Some(work),
        "the volume, not the empty folder on the boot disk"
    );
    assert_eq!(live.resolve(Path::new("/Volumes")).map(|id| live.parent(id)), Some(Some(0)));
    live.bury(work);
    live.settle();
    assert_eq!(live.resolve(Path::new("/Volumes/Work/notes.txt")), None);
    assert!(live.resolve(Path::new("/Volumes/Work")).is_some_and(|id| id != work), "the folder is all that is left");
}
