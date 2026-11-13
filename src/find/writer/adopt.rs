//! Plants apps the scan leaves out, like the Dock deep in the sealed system folders, so they can still be found. The
//! folders on the way to them are planted too.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::find::model::{Entry, Live, flag, with_tier};

use super::Writer;

const NOT_THE_USERS: u8 = 1;

impl Writer {
    pub(super) fn adopt(&mut self, apps: &[PathBuf]) {
        if !self.config.enabled || !self.launched {
            return;
        }
        let missing: Vec<&PathBuf> = {
            let live = self.shared.reading();
            apps.iter().filter(|app| !self.plan.wanted(app) && live.resolve(app).is_none()).collect()
        };
        let found: Vec<(&PathBuf, (u32, u32))> =
            missing.into_iter().filter_map(|app| Some((app, stamps(app)?))).collect();
        if found.is_empty() {
            return;
        }
        let mut live = self.shared.writing();
        let mut planted = 0;
        for (app, stamps) in found {
            let tier = self.plan.inside(app).max(NOT_THE_USERS);
            planted += usize::from(plant(&mut live, app, with_tier(flag::DIR, tier), stamps));
        }
        drop(live);
        self.timers.added += planted;
        self.shared.publish();
    }
}

fn stamps(app: &Path) -> Option<(u32, u32)> {
    let seen = std::fs::metadata(app).ok().filter(std::fs::Metadata::is_dir)?;
    let seconds = |at: std::io::Result<std::time::SystemTime>| {
        let since = at.ok().and_then(|at| at.duration_since(UNIX_EPOCH).ok());
        since.map_or(0, |since| u32::try_from(since.as_secs()).unwrap_or(u32::MAX))
    };
    Some((seconds(seen.modified()), seconds(seen.created())))
}

fn plant(live: &mut Live, path: &Path, flags: u8, (modified, created): (u32, u32)) -> bool {
    if live.resolve(path).is_some() {
        return false;
    }
    let mut missing = Vec::new();
    let mut at = path;
    let known = loop {
        let (Some(parent), Some(name)) = (at.parent(), at.file_name()) else { return false };
        missing.push(name.to_string_lossy().into_owned());
        if let Some(id) = live.resolve(parent).filter(|id| live.flags(*id) & flag::DIR != 0) {
            break id;
        }
        at = parent;
    };
    let mut parent = known;
    let last = missing.len() - 1;
    for (depth, name) in missing.iter().rev().enumerate() {
        let (modified, created) = if depth == last { (modified, created) } else { (0, 0) };
        parent = live.add(Some(parent), Entry { name, modified, created, flags });
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::find::model::tier;
    use crate::find::tests::corpus::from_paths;

    #[test]
    fn missing_app_is_planted() {
        let mut live = from_paths(&["/System/Library/", "/Applications/Maps.app/"]);
        let flags = with_tier(flag::DIR, 1);
        let wish = Path::new("/System/Library/Frameworks/Tk.framework/Versions/8.5/Resources/Wish.app");
        assert!(plant(&mut live, wish, flags, (50, 40)));
        let id = live.resolve(wish).expect("planted");
        assert_eq!((live.modified(id), live.created(id), tier(live.flags(id))), (50, 40, 1));
        let resources = live.resolve(wish.parent().unwrap()).expect("the folder above it");
        assert_eq!((live.modified(resources), live.flags(resources)), (0, flags), "a folder, and not the user's");
        assert!(!plant(&mut live, wish, flags, (60, 60)), "an app that is held is left as it is");
        assert_eq!(live.modified(id), 50);
        let dock = Path::new("/System/Library/CoreServices/Dock.app");
        assert!(plant(&mut live, dock, flags, (7, 7)));
        assert_eq!(live.parent(live.resolve(dock).unwrap()), live.resolve(Path::new("/System/Library/CoreServices")));
        assert!(!plant(&mut live, Path::new("/Applications/Maps.app"), flags, (1, 1)));
        assert!(!plant(&mut live, Path::new("Tool.app"), flags, (1, 1)), "a path with no root has nowhere to hang");
    }
}
