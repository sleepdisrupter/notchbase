//! The newest documents close to home, offered while Find has learned little.

use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::path::{Path, PathBuf};

use crate::find::context::Stamp;
use crate::find::kind::Kind;
use crate::find::model::{EntryId, Live, flag};
use crate::find::view::Why;

const HUMAN: [Kind; 8] =
    [Kind::Document, Kind::Pdf, Kind::Image, Kind::Video, Kind::Audio, Kind::Sheet, Kind::Slides, Kind::Archive];
const LEVELS_BELOW_HOME: usize = 4;
const HOME_IS_USUALLY_THIS_DEEP: usize = 2;
const NEVER_OFFERED: u8 = flag::DIR | flag::HIDDEN | flag::TIER_MASK | flag::INSIDE_PACKAGE | flag::LINK;

pub(super) fn human(folded_name: &[u8], flags: u8) -> bool {
    HUMAN.contains(&Kind::of(folded_name, flags))
}

pub(super) fn newest(
    live: &Live,
    now: Stamp,
    home: Option<&Path>,
    most: usize,
    wanted: &dyn Fn(&[u8], u8) -> bool,
) -> Vec<(EntryId, Why)> {
    let mut kept: BinaryHeap<Reverse<(Stamp, EntryId)>> = BinaryHeap::with_capacity(most + 1);
    let mut path = PathBuf::new();
    for id in 0..live.len() as EntryId {
        let flags = live.flags(id);
        if flags & NEVER_OFFERED != 0 || live.is_dead(id) {
            continue;
        }
        let time = live.modified(id).max(live.created(id));
        let oldest_kept = kept.peek().map_or(0, |Reverse((time, _))| *time);
        let newer = time > oldest_kept || (kept.len() < most && time != 0);
        if !newer || time > now || !wanted(live.folded(id), flags) || !close_to_home(live, id, home, &mut path) {
            continue;
        }
        kept.push(Reverse((time, id)));
        if kept.len() > most {
            kept.pop();
        }
    }
    let mut rows: Vec<(Stamp, EntryId)> = kept.into_iter().map(|Reverse(row)| row).collect();
    rows.sort_unstable_by(|a, b| b.cmp(a));
    rows.into_iter().map(|(_, id)| (id, told(live, id))).collect()
}

fn told(live: &Live, id: EntryId) -> Why {
    let (modified, created) = (live.modified(id), live.created(id));
    if created >= modified { Why::Added(created) } else { Why::Edited(modified) }
}

fn close_to_home(live: &Live, id: EntryId, home: Option<&Path>, path: &mut PathBuf) -> bool {
    let mut above = std::iter::successors(live.parent(id), |folder| live.parent(*folder));
    let mut levels = 0;
    let in_plain_sight = above.all(|folder| {
        levels += 1;
        live.flags(folder) & flag::HIDDEN == 0
    });
    let Some(home) = home else { return in_plain_sight && levels <= HOME_IS_USUALLY_THIS_DEEP + LEVELS_BELOW_HOME };
    live.path_into(id, path);
    in_plain_sight && path.strip_prefix(home).is_ok_and(|below| below.components().count() <= LEVELS_BELOW_HOME)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::find::model::with_tier;
    use crate::find::tests::corpus::{EPOCH, Planted, plant};

    const NOW: Stamp = EPOCH + 1_000;

    fn documents(name: &[u8], _flags: u8) -> bool {
        [&b".pdf"[..], b".png", b".docx"].iter().any(|ending| name.ends_with(ending))
    }

    fn planted(files: &[(&str, u32, u32, u8)]) -> Live {
        let mut live = Live::empty();
        for (path, modified, created, flags) in files {
            plant(&mut live, &Planted { path, modified: *modified, created: *created, flags: *flags });
        }
        live
    }

    fn names(live: &Live, rows: &[(EntryId, Why)]) -> Vec<String> {
        let mut name = String::new();
        rows.iter()
            .map(|(id, _)| {
                live.name_into(*id, &mut name);
                name.clone()
            })
            .collect()
    }

    #[test]
    fn newest_documents_first() {
        let live = planted(&[
            ("/Users/john/Documents/old.pdf", EPOCH, EPOCH, 0),
            ("/Users/john/Documents/changed.docx", EPOCH + 300, EPOCH, 0),
            ("/Users/john/Downloads/arrived.png", EPOCH + 100, EPOCH + 500, 0),
            ("/Users/john/Documents/main.rs", EPOCH + 900, EPOCH + 900, 0),
        ]);
        let rows = newest(&live, NOW, Some(Path::new("/Users/john")), 2, &documents);
        assert_eq!(names(&live, &rows), ["arrived.png", "changed.docx"]);
        assert_eq!(rows[0].1, Why::Added(EPOCH + 500));
        assert_eq!(rows[1].1, Why::Edited(EPOCH + 300));
        assert_eq!(newest(&live, NOW, Some(Path::new("/Users/john")), 20, &documents).len(), 3);
        assert!(newest(&live, NOW, Some(Path::new("/Users/john")), 0, &documents).is_empty());
    }

    #[test]
    fn junk_and_future_never_offered() {
        let live = planted(&[
            ("/Users/john/Library/Caches/thumb.png", EPOCH + 9, EPOCH + 9, with_tier(0, 3)),
            ("/Users/john/.secret.pdf", EPOCH + 8, EPOCH + 8, flag::HIDDEN),
            ("/Users/john/.config/", EPOCH, EPOCH, flag::HIDDEN),
            ("/Users/john/.config/inside.pdf", EPOCH + 7, EPOCH + 7, 0),
            ("/Users/john/Scans.pdf/", EPOCH + 6, EPOCH + 6, 0),
            ("/Users/john/App.app/Contents/icon.png", EPOCH + 5, EPOCH + 5, flag::INSIDE_PACKAGE),
            ("/Users/john/from-2040.pdf", NOW + 5, NOW + 5, 0),
            ("/Users/john/undated.pdf", 0, 0, 0),
            ("/Users/john/fine.pdf", EPOCH + 1, EPOCH + 1, 0),
        ]);
        let rows = newest(&live, NOW, Some(Path::new("/Users/john")), 20, &documents);
        assert_eq!(names(&live, &rows), ["fine.pdf"]);
    }

    #[test]
    fn only_near_home_offered() {
        let live = planted(&[
            ("/Users/john/a/b/c/near.pdf", EPOCH + 1, EPOCH + 1, 0),
            ("/Users/john/a/b/c/d/deep.pdf", EPOCH + 2, EPOCH + 2, 0),
            ("/Users/Shared/other.pdf", EPOCH + 3, EPOCH + 3, 0),
        ]);
        let rows = newest(&live, NOW, Some(Path::new("/Users/john")), 20, &documents);
        assert_eq!(names(&live, &rows), ["near.pdf"]);
        let homeless = newest(&live, NOW, None, 20, &documents);
        assert_eq!(
            names(&live, &homeless),
            ["other.pdf", "near.pdf"],
            "six levels below the root when home is unknown"
        );
    }

    #[test]
    fn buried_file_not_offered() {
        let mut live =
            planted(&[("/Users/john/gone.pdf", EPOCH + 2, EPOCH + 2, 0), ("/Users/john/here.pdf", EPOCH, EPOCH, 0)]);
        let gone = live.resolve(Path::new("/Users/john/gone.pdf")).unwrap();
        live.bury(gone);
        live.settle();
        let rows = newest(&live, NOW, None, 20, &documents);
        assert_eq!(names(&live, &rows), ["here.pdf"]);
    }

    #[test]
    fn human_kinds_are_kept_ones() {
        assert_eq!(HUMAN.len(), 8);
        assert!(!HUMAN.contains(&Kind::Code) && !HUMAN.contains(&Kind::Folder) && !HUMAN.contains(&Kind::Other));
        assert!(!human(b"notes", flag::DIR));
    }
}
