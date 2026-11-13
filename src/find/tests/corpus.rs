use std::path::{Path, PathBuf};

use crate::find::model::{Entry, EntryId, Live, flag};

pub const ROOT: &str = "/";
pub const EPOCH: u32 = 1_700_000_000;
pub const SPAN: u32 = 30 * 24 * 60 * 60;

const FOLDER_SHARE: u64 = 4;
const NEAR_SHARE: u64 = 3;
const NEAR: usize = 64;
const STEMS: [&str; 16] = [
    "report", "notes", "index", "main", "config", "photo", "draft", "invoice", "readme", "budget", "sketch", "plan",
    "export", "backup", "cargo", "summary",
];
const ENDINGS: [&str; 8] = ["txt", "md", "rs", "pdf", "png", "json", "toml", "mov"];

pub struct Noise(u64);

impl Noise {
    pub fn seeded(seed: u64) -> Self {
        Self(seed | 1)
    }

    pub fn roll(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    pub fn below(&mut self, limit: usize) -> usize {
        (self.roll() % limit.max(1) as u64) as usize
    }
}

pub struct Planted<'a> {
    pub path: &'a str,
    pub modified: u32,
    pub created: u32,
    pub flags: u8,
}

pub fn plant(live: &mut Live, planted: &Planted<'_>) -> EntryId {
    let root = live.resolve(Path::new(ROOT)).unwrap_or_else(|| live.add(None, folder(ROOT)));
    let steps: Vec<&str> = planted.path.split('/').filter(|step| !step.is_empty()).collect();
    let mut at = (root, PathBuf::from(ROOT));
    for (depth, step) in steps.iter().enumerate() {
        at.1.push(step);
        let last = depth + 1 == steps.len();
        let entry = if last {
            let kind = if planted.path.ends_with('/') { flag::DIR } else { 0 };
            Entry { name: step, modified: planted.modified, created: planted.created, flags: planted.flags | kind }
        } else {
            folder(step)
        };
        at.0 = live.resolve(&at.1).unwrap_or_else(|| live.add(Some(at.0), entry));
    }
    at.0
}

pub fn from_paths(paths: &[&str]) -> Live {
    let mut live = Live::empty();
    for path in paths {
        plant(&mut live, &Planted { path, modified: EPOCH, created: EPOCH, flags: 0 });
    }
    live
}

pub fn grown(seed: u64, entries: usize) -> Live {
    let mut noise = Noise::seeded(seed);
    let mut live = Live::empty();
    let mut folders = vec![live.add(None, folder(ROOT))];
    while live.len() < entries {
        let near = folders.len() - 1 - noise.below(folders.len().min(NEAR));
        let parent = if noise.roll().is_multiple_of(NEAR_SHARE) { noise.below(folders.len()) } else { near };
        let stem = STEMS[noise.below(STEMS.len())];
        let serial = live.len();
        let modified = EPOCH + (noise.roll() % u64::from(SPAN)) as u32;
        if noise.roll().is_multiple_of(FOLDER_SHARE) {
            let name = format!("{stem}-{serial}");
            folders.push(live.add(Some(folders[parent]), Entry { modified, ..folder(&name) }));
        } else {
            let name = format!("{stem}-{serial}.{}", ENDINGS[noise.below(ENDINGS.len())]);
            live.add(Some(folders[parent]), Entry { name: &name, modified, created: modified, flags: 0 });
        }
    }
    live
}

fn folder(name: &str) -> Entry<'_> {
    Entry { name, modified: EPOCH, created: EPOCH, flags: flag::DIR }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(live: &Live) -> Vec<String> {
        let mut path = PathBuf::new();
        (0..live.len() as EntryId)
            .map(|id| {
                live.path_into(id, &mut path);
                format!("{} {} {}", path.display(), live.modified(id), live.flags(id))
            })
            .collect()
    }

    #[test]
    fn same_seed_same_tree() {
        let (first, again, other) = (grown(7, 2_000), grown(7, 2_000), grown(8, 2_000));
        assert_eq!(first.len(), 2_000);
        assert_eq!(names(&first), names(&again));
        assert_ne!(names(&first), names(&other));
    }

    #[test]
    fn grown_tree_has_depth() {
        let live = grown(42, 5_000);
        let folders = (0..live.len() as EntryId).filter(|id| live.flags(*id) & flag::DIR != 0).count();
        assert!((800..2_000).contains(&folders), "{folders} folders");
        let deepest =
            (0..live.len() as EntryId).map(|id| std::iter::successors(Some(id), |id| live.parent(*id)).count()).max();
        assert!(deepest.is_some_and(|depth| depth >= 6), "{deepest:?} levels");
        assert!((1..live.len() as EntryId).all(|id| live.parent(id).is_some_and(|parent| parent < id)));
    }

    #[test]
    fn planting_adds_folders_once() {
        let mut live = from_paths(&["/Users/john/a.txt", "/Users/john/Pictures/", "/Users/john/a.txt"]);
        assert_eq!(live.len(), 5, "the root, two folders above, a file and a folder");
        let pictures = live.resolve(Path::new("/Users/john/Pictures")).unwrap();
        assert_ne!(live.flags(pictures) & flag::DIR, 0);
        let file = live.resolve(Path::new("/Users/john/a.txt")).unwrap();
        assert_eq!((live.flags(file), live.modified(file)), (0, EPOCH));
        let hidden = Planted { path: "/Users/john/.zshrc", modified: 9, created: 4, flags: flag::HIDDEN };
        let zshrc = plant(&mut live, &hidden);
        assert_eq!((live.flags(zshrc), live.modified(zshrc), live.created(zshrc)), (flag::HIDDEN, 9, 4));
        assert_eq!(plant(&mut live, &hidden), zshrc);
    }
}
