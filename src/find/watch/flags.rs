//! Reads FSEvents flags on macOS. Item events become hints to look at a path, dropped events rescan, wrapped ids and
//! mounts start over, and two renames in a row make one move.

use std::path::PathBuf;

use super::{Burst, Change, Event};

pub mod fired {
    pub const MUST_SCAN_SUB_DIRS: u32 = 0x0000_0001;
    pub const USER_DROPPED: u32 = 0x0000_0002;
    pub const KERNEL_DROPPED: u32 = 0x0000_0004;
    pub const EVENT_IDS_WRAPPED: u32 = 0x0000_0008;
    pub const HISTORY_DONE: u32 = 0x0000_0010;
    pub const ROOT_CHANGED: u32 = 0x0000_0020;
    pub const MOUNT: u32 = 0x0000_0040;
    pub const UNMOUNT: u32 = 0x0000_0080;
    #[cfg(test)]
    pub const ITEM_CREATED: u32 = 0x0000_0100;
    #[cfg(test)]
    pub const ITEM_REMOVED: u32 = 0x0000_0200;
    pub const ITEM_RENAMED: u32 = 0x0000_0800;
    #[cfg(test)]
    pub const ITEM_MODIFIED: u32 = 0x0000_1000;
    #[cfg(test)]
    pub const ITEM_IS_FILE: u32 = 0x0001_0000;
    #[cfg(test)]
    pub const ITEM_IS_DIR: u32 = 0x0002_0000;
    pub const RESCAN: u32 = MUST_SCAN_SUB_DIRS | USER_DROPPED | KERNEL_DROPPED;
    pub const VOLUMES: u32 = MOUNT | UNMOUNT;
}

const DATA_VOLUME: &str = "/System/Volumes/Data";
const SEPARATOR: char = '/';

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fired {
    pub path: PathBuf,
    pub flags: u32,
    pub id: u64,
}

impl Fired {
    pub fn new(path: &str, flags: u32, id: u64) -> Self {
        Self { path: normalised(path), flags, id }
    }
}

/// FSEvents can report paths under /System/Volumes/Data, and the index knows them by their firmlinked form.
pub fn normalised(path: &str) -> PathBuf {
    let firmlinked = match path.strip_prefix(DATA_VOLUME) {
        Some("") => "/",
        Some(rest) if rest.starts_with(SEPARATOR) => rest,
        _ => path,
    };
    let trimmed = firmlinked.trim_end_matches(SEPARATOR);
    PathBuf::from(if trimmed.is_empty() { "/" } else { trimmed })
}

pub fn burst(fired: &[Fired], roots: &[PathBuf]) -> Burst {
    let mut out = Burst::default();
    let mut index = 0;
    while let Some(event) = fired.get(index) {
        index += 1;
        let flags = event.flags;
        let path = event.path.clone();
        let changes: Vec<Change> = if flags & fired::HISTORY_DONE != 0 {
            out.caught_up = true;
            Vec::new()
        } else if flags & fired::EVENT_IDS_WRAPPED != 0 {
            roots.iter().cloned().map(Change::Everything).collect()
        } else if flags & fired::ROOT_CHANGED != 0 {
            vec![Change::Everything(path)]
        } else if flags & fired::VOLUMES != 0 {
            vec![Change::Overflow(path)]
        } else if flags & fired::RESCAN != 0 {
            rescan(path, roots)
        } else if let Some(next) = fired.get(index).filter(|next| renamed(event) && renamed(next)) {
            index += 1;
            let moved = Change::Moved { from: path, to: next.path.clone() };
            out.events.push(Event { change: moved, id: event.id.min(next.id) });
            Vec::new()
        } else {
            vec![Change::Touched(path)]
        };
        out.events.extend(changes.into_iter().map(|change| Event { change, id: event.id }));
    }
    out
}

fn renamed(event: &Fired) -> bool {
    let special = fired::HISTORY_DONE | fired::EVENT_IDS_WRAPPED | fired::ROOT_CHANGED | fired::VOLUMES | fired::RESCAN;
    event.flags & fired::ITEM_RENAMED != 0 && event.flags & special == 0
}

fn rescan(path: PathBuf, roots: &[PathBuf]) -> Vec<Change> {
    let covered: Vec<Change> =
        roots.iter().filter(|root| root.starts_with(&path)).cloned().map(Change::Overflow).collect();
    if covered.is_empty() { vec![Change::Subtree(path)] } else { covered }
}

#[cfg(test)]
mod tests {
    use super::fired::*;
    use super::*;

    fn roots() -> Vec<PathBuf> {
        vec![PathBuf::from("/"), PathBuf::from("/Volumes/Work")]
    }

    fn one(path: &str, flags: u32) -> Vec<Change> {
        burst(&[Fired::new(path, flags, 40)], &roots()).events.into_iter().map(|event| event.change).collect()
    }

    fn touched(path: &str) -> Vec<Change> {
        vec![Change::Touched(PathBuf::from(path))]
    }

    #[test]
    fn item_events_are_hints() {
        let notes = "/Users/me/notes.txt";
        assert_eq!(one(notes, ITEM_CREATED | ITEM_IS_FILE), touched(notes));
        assert_eq!(one(notes, ITEM_MODIFIED | ITEM_IS_FILE), touched(notes));
        assert_eq!(one(notes, ITEM_REMOVED | ITEM_IS_FILE), touched(notes));
        assert_eq!(one("/Users/me/Projects", ITEM_CREATED | ITEM_IS_DIR), touched("/Users/me/Projects"));
        assert_eq!(one(notes, ITEM_RENAMED), touched(notes), "half a rename");
        assert_eq!(one(notes, 0), touched(notes));
    }

    #[test]
    fn dropped_events_rescan() {
        assert_eq!(one("/Users/me/src", MUST_SCAN_SUB_DIRS), [Change::Subtree("/Users/me/src".into())]);
        assert_eq!(one("/Volumes/Work", USER_DROPPED), [Change::Overflow("/Volumes/Work".into())]);
        assert_eq!(one("/", KERNEL_DROPPED), roots().into_iter().map(Change::Overflow).collect::<Vec<_>>());
    }

    #[test]
    fn wraps_and_mounts_start_over() {
        assert_eq!(one("/", EVENT_IDS_WRAPPED), roots().into_iter().map(Change::Everything).collect::<Vec<_>>());
        assert_eq!(one("/Volumes/Work", ROOT_CHANGED), [Change::Everything("/Volumes/Work".into())]);
        assert_eq!(one("/Volumes/Camera", MOUNT), [Change::Overflow("/Volumes/Camera".into())]);
        assert_eq!(one("/Volumes/Camera", UNMOUNT | ITEM_IS_DIR), [Change::Overflow("/Volumes/Camera".into())]);
    }

    #[test]
    fn history_done_means_caught_up() {
        let replayed = burst(&[Fired::new("/", HISTORY_DONE, 0)], &roots());
        assert_eq!(replayed, Burst { events: Vec::new(), caught_up: true });
        assert!(!burst(&[Fired::new("/a", ITEM_CREATED, 1)], &roots()).caught_up);
    }

    #[test]
    fn two_renames_make_a_move() {
        let fired = [
            Fired::new("/Users/me/old", ITEM_RENAMED | ITEM_IS_DIR, 71),
            Fired::new("/Users/me/new", ITEM_RENAMED | ITEM_IS_DIR, 72),
            Fired::new("/Users/me/lonely", ITEM_RENAMED, 73),
            Fired::new("/Users/me/after", ITEM_CREATED, 74),
        ];
        let moved = Change::Moved { from: "/Users/me/old".into(), to: "/Users/me/new".into() };
        let expected = vec![
            Event { change: moved, id: 71 },
            Event { change: Change::Touched("/Users/me/lonely".into()), id: 73 },
            Event { change: Change::Touched("/Users/me/after".into()), id: 74 },
        ];
        assert_eq!(burst(&fired, &roots()).events, expected);
    }

    #[test]
    fn rename_beside_drop_stays_alone() {
        let fired = [Fired::new("/a", ITEM_RENAMED, 1), Fired::new("/b", ITEM_RENAMED | MUST_SCAN_SUB_DIRS, 2)];
        let changes: Vec<Change> = burst(&fired, &roots()).events.into_iter().map(|event| event.change).collect();
        assert_eq!(changes, [Change::Touched("/a".into()), Change::Subtree("/b".into())]);
    }

    #[test]
    fn data_volume_paths_are_firmlinked() {
        assert_eq!(normalised("/System/Volumes/Data/Users/me/a.txt"), PathBuf::from("/Users/me/a.txt"));
        assert_eq!(normalised("/System/Volumes/Data"), PathBuf::from("/"));
        assert_eq!(normalised("/System/Volumes/DataStore/x"), PathBuf::from("/System/Volumes/DataStore/x"));
        assert_eq!(normalised("/Users/me/folder/"), PathBuf::from("/Users/me/folder"));
        assert_eq!(normalised("/"), PathBuf::from("/"));
    }
}
