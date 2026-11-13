use std::path::{Path, PathBuf};

use crate::find::key::{START, chain, key_of};
use crate::find::model::{Entry, EntryId, Live, Room, flag, fold_into};

use super::corpus::{from_paths, grown};

const ROOM: Room = Room { cap: usize::MAX, level: 0, boot: None };

fn file(name: &str) -> Entry<'_> {
    Entry { name, modified: 10, created: 5, flags: 0 }
}

fn folder(name: &str) -> Entry<'_> {
    Entry { name, modified: 10, created: 5, flags: flag::DIR }
}

fn path_of(live: &Live, id: EntryId) -> PathBuf {
    let mut path = PathBuf::from("left over from the last caller");
    live.path_into(id, &mut path);
    path
}

fn name_of(live: &Live, id: EntryId) -> String {
    let mut name = String::from("left over");
    live.name_into(id, &mut name);
    name
}

fn children(live: &Live, dir: EntryId) -> Vec<String> {
    let mut ids = vec![EntryId::MAX];
    live.children_into(dir, &mut ids);
    ids.into_iter().map(|id| name_of(live, id)).collect()
}

fn found(live: &Live, path: &str) -> EntryId {
    live.resolve(Path::new(path)).unwrap_or_else(|| panic!("{path} is in the index"))
}

fn rebuild(live: &mut Live) {
    let rebuilt = live.rebuilt(&ROOM);
    live.adopt(rebuilt);
}

#[test]
fn arena_bytes_lead_to_entry() {
    let mut live = grown(3, 400);
    live.add(Some(0), file("Caf\u{e9} M\u{dc}NCHEN.txt"));
    let mut seen = 0;
    for (index, arena) in live.arenas().into_iter().enumerate() {
        let names: Vec<&[u8]> = match arena.bytes {
            [0] => Vec::new(),
            [0, names @ .., 0] => names.split(|byte| *byte == 0).collect(),
            other => panic!("arena {index} is not NUL, name, NUL: {} bytes", other.len()),
        };
        let mut offset = 1;
        for (nth, name) in names.into_iter().enumerate() {
            let id = arena.first + nth as EntryId;
            let mut folded = Vec::new();
            let exotic = fold_into(&name_of(&live, id), &mut folded);
            assert_eq!((live.folded(id), name), (&folded[..], &folded[..]));
            assert_eq!(live.flags(id) & flag::EXOTIC != 0, exotic, "exotic exactly when the fold lost something");
            assert!((offset..offset + name.len()).all(|at| live.id_at(index, at) == id));
            offset += name.len() + 1;
            seen += 1;
        }
    }
    assert_eq!(seen, live.len(), "every entry has exactly one name in one arena");
}

#[test]
fn ids_count_up_children_after() {
    let mut live = grown(11, 600);
    assert_eq!((live.len(), live.alive()), (600, 600));
    assert!((0..600).all(|id| live.parent(id).is_none_or(|parent| parent < id)));
    let first = live.add(Some(0), file("late.txt"));
    assert_eq!((first, live.len()), (600, 601));
    live.bury(first);
    assert_eq!((live.len(), live.alive(), live.is_dead(first)), (601, 600, true), "a dead row keeps its id");
}

#[test]
fn root_holds_whole_prefix() {
    let mut live = Live::empty();
    for prefix in ["/", "/Volumes/Work", "C:\\"] {
        let root = live.add(None, folder(prefix));
        let users = live.add(Some(root), folder("Users"));
        let notes = live.add(Some(users), file("notes.txt"));
        assert_eq!((live.parent(root), name_of(&live, root)), (None, prefix.to_owned()));
        assert_eq!(path_of(&live, root), Path::new(prefix));
        assert_eq!(path_of(&live, notes), Path::new(prefix).join("Users").join("notes.txt"));
        assert_eq!(name_of(&live, notes), "notes.txt");
        for id in [root, users, notes] {
            assert_eq!(live.resolve(&path_of(&live, id)), Some(id), "{prefix}");
            assert_eq!(live.key(id), key_of(&path_of(&live, id)), "{prefix}");
        }
        assert_eq!(live.key(notes), chain(chain(key_of(Path::new(prefix)), "Users"), "notes.txt"));
    }
    assert_eq!(live.key(found(&live, "/")), START);
    assert_eq!(live.resolve(Path::new("/Volumes/Work/Users")).map(|id| live.parent(id)), Some(Some(3)));
    assert_eq!(live.resolve(Path::new("/Users/missing.txt")), None);
    assert_eq!(live.resolve(Path::new("relative/notes.txt")), None);
}

#[test]
fn path_matched_byte_for_byte() {
    let live = from_paths(&["/Users/John/Makefile", "/Users/John/makefile", "/Users/John/r\u{e9}sum\u{e9}.pdf"]);
    let (upper, lower) = (found(&live, "/Users/John/Makefile"), found(&live, "/Users/John/makefile"));
    assert_ne!(upper, lower, "two names that fold alike are two entries");
    assert_eq!((name_of(&live, upper), name_of(&live, lower)), ("Makefile".to_owned(), "makefile".to_owned()));
    assert_eq!(live.folded(upper), live.folded(lower));
    assert_eq!(live.resolve(Path::new("/users/john/makefile")), None);
    assert_eq!(live.resolve(Path::new("/Users/John/MAKEFILE")), None);
    assert_eq!(live.resolve(Path::new("/Users/John/resume.pdf")), None);
    assert_eq!(name_of(&live, found(&live, "/Users/John/r\u{e9}sum\u{e9}.pdf")), "r\u{e9}sum\u{e9}.pdf");
}

#[test]
fn siblings_ordered_by_fold() {
    let mut live =
        from_paths(&["/d/beta.txt", "/d/Alpha.txt", "/d/makefile", "/d/Makefile", "/d/alpha.txt", "/d/sub/x"]);
    let dir = found(&live, "/d");
    assert_eq!(children(&live, dir), ["Alpha.txt", "alpha.txt", "beta.txt", "Makefile", "makefile", "sub"]);
    live.bury(found(&live, "/d/beta.txt"));
    assert_eq!(children(&live, dir), ["Alpha.txt", "alpha.txt", "Makefile", "makefile", "sub"]);
    assert_eq!(children(&live, found(&live, "/d/Makefile")), Vec::<String>::new());
}

#[test]
fn rename_is_burial_and_add() {
    let mut live = from_paths(&["/d/old.txt", "/d/other.txt"]);
    let (dir, old) = (found(&live, "/d"), found(&live, "/d/old.txt"));
    live.bury(old);
    let new = live.add(Some(dir), file("new.txt"));
    assert_eq!((live.resolve(Path::new("/d/old.txt")), live.resolve(Path::new("/d/new.txt"))), (None, Some(new)));
    assert!(live.is_dead(old) && !live.is_dead(new) && new > old);
    let back = live.add(Some(dir), file("old.txt"));
    assert_eq!(live.resolve(Path::new("/d/old.txt")), Some(back), "the name can come back as a new entry");
    assert_eq!(live.alive(), 5);
}

#[test]
fn burying_folder_hides_below() {
    let mut live = from_paths(&["/keep/a.txt", "/gone/deep/er/b.txt", "/gone/c.txt"]);
    let (gone, deepest) = (found(&live, "/gone"), found(&live, "/gone/deep/er/b.txt"));
    let before = live.alive();
    live.bury(gone);
    live.settle();
    assert!(live.is_dead(gone) && live.is_dead(deepest));
    assert_eq!(live.alive(), before - 5);
    assert_eq!(live.resolve(Path::new("/gone/deep/er/b.txt")), None);
    assert_eq!(live.resolve(Path::new("/gone")), None);
    assert_eq!(children(&live, found(&live, "/")), ["keep"]);
    assert!(found(&live, "/keep/a.txt") > 0);
    live.settle();
    assert_eq!(live.alive(), before - 5, "settling twice changes nothing");
}

#[test]
fn save_and_tier_keep_row() {
    let mut live = Live::empty();
    let root = live.add(None, folder("/"));
    let link = live.add(Some(root), Entry { flags: flag::LINK | flag::HIDDEN, ..file("alias") });
    live.touch(link, 99);
    assert_eq!((live.modified(link), live.created(link)), (99, 5));
    live.reflag(link, flag::DIR | flag::CLOUD | flag::EXOTIC | (2 << flag::TIER_SHIFT));
    assert_eq!(
        live.flags(link),
        flag::LINK | flag::CLOUD | (2 << flag::TIER_SHIFT),
        "hidden cleared, tier and cloud set"
    );
    live.reflag(root, flag::HIDDEN);
    assert_eq!(live.flags(root), flag::DIR | flag::HIDDEN);
    assert_eq!(live.flags(link) & flag::EXOTIC, 0, "a plain ASCII name is never exotic");
}

#[test]
fn revision_and_generation_rise() {
    let mut live = Live::empty();
    let mut last = (live.generation(), live.revision());
    let mut rose = |live: &Live, what: &str| {
        assert!(live.revision() > last.1, "{what} is a change");
        let generation = live.generation();
        last = (generation, live.revision());
        generation
    };
    let root = live.add(None, folder("/"));
    assert_eq!(rose(&live, "adding"), 0);
    let file_id = live.add(Some(root), file("a.txt"));
    live.touch(file_id, 77);
    assert_eq!(rose(&live, "a save"), 0);
    live.reflag(file_id, flag::HIDDEN);
    assert_eq!(rose(&live, "a new flag"), 0);
    live.bury(file_id);
    assert_eq!(rose(&live, "a burial"), 0);
    rebuild(&mut live);
    assert_eq!(rose(&live, "a rebuild"), 1);
    rebuild(&mut live);
    assert_eq!(live.generation(), 2);
}

#[test]
fn settled_means_nothing_pending() {
    let mut live = Live::empty();
    assert!(live.settled(), "an empty index has nothing to merge");
    let root = live.add(None, folder("/"));
    assert!(!live.settled());
    rebuild(&mut live);
    assert!(live.settled());
    live.bury(root);
    assert!(!live.settled());
    rebuild(&mut live);
    assert!(live.settled() && live.len() == 0);
}

#[test]
fn rebuild_drops_only_dead() {
    let mut live = grown(5, 800);
    let doomed: Vec<EntryId> = (1..800).filter(|id: &EntryId| id.is_multiple_of(7)).collect();
    doomed.iter().for_each(|id| live.bury(*id));
    live.settle();
    let late = live.add(Some(0), Entry { flags: flag::HIDDEN | flag::CLOUD, ..file("Late Arrival.TXT") });
    live.touch(late, 1234);
    let survivors: Vec<(PathBuf, u32, u32, u8)> = (0..live.len() as EntryId)
        .filter(|id| !live.is_dead(*id))
        .map(|id| (path_of(&live, id), live.modified(id), live.created(id), live.flags(id)))
        .collect();
    let gone: Vec<PathBuf> = doomed.iter().map(|id| path_of(&live, *id)).collect();
    rebuild(&mut live);
    assert_eq!((live.len(), live.alive()), (survivors.len(), survivors.len()));
    for (path, modified, created, flags) in &survivors {
        let id = live.resolve(path).unwrap_or_else(|| panic!("{} survived", path.display()));
        assert_eq!((live.modified(id), live.created(id), live.flags(id)), (*modified, *created, *flags));
        assert_eq!((path_of(&live, id), live.key(id)), (path.clone(), key_of(path)));
    }
    assert!(gone.iter().all(|path| live.resolve(path).is_none()));
    assert!((0..live.len() as EntryId).all(|id| !live.is_dead(id) && live.parent(id).is_none_or(|parent| parent < id)));
    assert_eq!(name_of(&live, found(&live, "/Late Arrival.TXT")), "Late Arrival.TXT");
}
