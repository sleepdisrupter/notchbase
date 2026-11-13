//! Watching the disk. The platform watcher turns what the system reports into numbered `Change` events, and a
//! `Watcher` stops watching when it is dropped.

pub mod coalesce;
#[cfg(any(test, target_os = "macos"))]
pub mod flags;
mod lane;
#[cfg(any(test, target_os = "windows"))]
pub mod notify;

use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    Touched(PathBuf),
    Relist(PathBuf),
    Subtree(PathBuf),
    #[cfg_attr(not(any(test, target_os = "macos", windows)), expect(dead_code))]
    Moved {
        from: PathBuf,
        to: PathBuf,
    },
    #[cfg_attr(not(any(test, target_os = "macos", windows)), expect(dead_code))]
    Overflow(PathBuf),
    #[cfg_attr(not(any(test, target_os = "macos")), expect(dead_code))]
    Everything(PathBuf),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub change: Change,
    pub id: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Burst {
    pub events: Vec<Event>,
    pub caught_up: bool,
}

pub struct Watcher {
    stop: Option<Box<dyn FnOnce() + Send>>,
}

impl Watcher {
    #[cfg_attr(not(any(test, target_os = "macos", windows)), expect(dead_code))]
    pub fn new(stop: impl FnOnce() + Send + 'static) -> Self {
        Self { stop: Some(Box::new(stop)) }
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            stop();
        }
    }
}

impl std::fmt::Debug for Watcher {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("Watcher").field("watching", &self.stop.is_some()).finish()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[test]
    fn drop_stops_the_watcher_once() {
        let stops = Arc::new(AtomicUsize::new(0));
        let counter = stops.clone();
        let watcher = Watcher::new(move || {
            counter.fetch_add(1, Ordering::Relaxed);
        });
        assert_eq!(stops.load(Ordering::Relaxed), 0, "watching until it is let go");
        drop(watcher);
        assert_eq!(stops.load(Ordering::Relaxed), 1);
    }
}
