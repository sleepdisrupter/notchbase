//! Watches the tools folders for rebuilt plugins, checked once a second and reported once the files stop changing.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use super::plugin;

const CHECK_EVERY: Duration = Duration::from_secs(1);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stamp {
    path: PathBuf,
    len: u64,
    modified: Option<SystemTime>,
}

impl Stamp {
    fn of(path: PathBuf) -> Self {
        let meta = std::fs::metadata(&path).ok();
        let len = meta.as_ref().map_or(0, std::fs::Metadata::len);
        Self { modified: meta.and_then(|meta| meta.modified().ok()), len, path }
    }

    pub fn name(&self) -> String {
        self.path.parent().and_then(Path::file_name).unwrap_or_default().to_string_lossy().into_owned()
    }
}

pub fn stamps(dirs: &[PathBuf]) -> Vec<Stamp> {
    dirs.iter().flat_map(|dir| plugin::libraries(dir)).map(Stamp::of).collect()
}

#[derive(Debug, Default)]
pub struct Watch {
    loaded: Vec<Stamp>,
    seen: Vec<Stamp>,
    checked_at: Option<Instant>,
}

impl Watch {
    pub fn new(loaded: Vec<Stamp>) -> Self {
        Self { seen: loaded.clone(), loaded, checked_at: None }
    }

    pub fn changed(&mut self, now: Instant, look: impl FnOnce() -> Vec<Stamp>) -> Option<Vec<String>> {
        if self.checked_at.is_some_and(|at| now.duration_since(at) < CHECK_EVERY) {
            return None;
        }
        self.checked_at = Some(now);
        let current = look();
        let settled = current == self.seen;
        self.seen = current;
        if !settled || self.seen == self.loaded {
            return None;
        }
        let names = self.seen.iter().filter(|stamp| !self.loaded.contains(stamp)).map(Stamp::name).collect();
        self.loaded = self.seen.clone();
        Some(names)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stamp(name: &str, len: u64) -> Stamp {
        Stamp { path: PathBuf::from("tools").join(name).join("lib.dll"), len, modified: None }
    }

    #[test]
    fn rebuilt_plugin_reloads_once_settled() {
        let t0 = Instant::now();
        let mut watch = Watch::new(vec![stamp("notes", 10)]);
        assert_eq!(watch.changed(t0, || vec![stamp("notes", 10)]), None);
        assert_eq!(watch.changed(t0 + CHECK_EVERY, || vec![stamp("notes", 4)]), None, "still being written");
        assert_eq!(watch.changed(t0 + CHECK_EVERY / 2 * 3, || vec![stamp("notes", 20)]), None, "too soon to look");
        assert_eq!(watch.changed(t0 + CHECK_EVERY * 2, || vec![stamp("notes", 20)]), None);
        assert_eq!(watch.changed(t0 + CHECK_EVERY * 3, || vec![stamp("notes", 20)]), Some(vec!["notes".to_owned()]));
        assert_eq!(watch.changed(t0 + CHECK_EVERY * 4, || vec![stamp("notes", 20)]), None);
    }

    #[test]
    fn late_watch_has_nothing_to_report() {
        let t0 = Instant::now();
        let mut stale = Watch::new(vec![stamp("notes", 10)]);
        let mut fresh = Watch::new(vec![stamp("notes", 20), stamp("weather", 1)]);
        let on_disk = || vec![stamp("notes", 20), stamp("weather", 1)];
        for step in 0..4 {
            assert_eq!(fresh.changed(t0 + CHECK_EVERY * step, on_disk), None, "whoever reloaded already said so");
        }
        stale.changed(t0, on_disk);
        assert!(stale.changed(t0 + CHECK_EVERY, on_disk).is_some(), "the same files are news to an older watch");
    }

    #[test]
    fn new_or_removed_plugin_counts() {
        let t0 = Instant::now();
        let mut watch = Watch::new(Vec::new());
        watch.changed(t0, || vec![stamp("weather", 1)]);
        assert_eq!(watch.changed(t0 + CHECK_EVERY, || vec![stamp("weather", 1)]), Some(vec!["weather".to_owned()]));
        watch.changed(t0 + CHECK_EVERY * 2, Vec::new);
        assert_eq!(watch.changed(t0 + CHECK_EVERY * 3, Vec::new), Some(Vec::new()));
    }
}
