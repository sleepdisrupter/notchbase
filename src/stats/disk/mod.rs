//! Storage: where the disk's space goes, by part. Four workers measure the home folder and a few system folders, and
//! the view fills in after each root. A scan is good for ten minutes.

pub(crate) mod plan;
pub(crate) mod scan;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use super::{Wake, lock};
use scan::Tally;

const STALE_AFTER: Duration = Duration::from_secs(10 * 60);
const WORKERS: usize = 4;
const ITEMS_KEPT: usize = 30;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Apps,
    Documents,
    Desktop,
    Downloads,
    Media,
    Music,
    Machines,
    Developer,
    AppData,
    Other,
    System,
}

impl Part {
    pub const ALL: [Self; 11] = [
        Self::Apps,
        Self::Documents,
        Self::Desktop,
        Self::Downloads,
        Self::Media,
        Self::Music,
        Self::Machines,
        Self::Developer,
        Self::AppData,
        Self::Other,
        Self::System,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::Apps => "Apps",
            Self::Documents => "Documents",
            Self::Desktop => "Desktop",
            Self::Downloads => "Downloads",
            Self::Media => "Photos and video",
            Self::Music => "Music",
            Self::Machines => "Virtual machines",
            Self::Developer => "Developer",
            Self::AppData => "App data",
            Self::Other => "Other files",
            Self::System => "System and other",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Item {
    pub name: String,
    pub path: PathBuf,
    pub size: u64,
    pub folder: bool,
    pub icon: Option<Arc<egui::ColorImage>>,
}

#[derive(Clone, Debug)]
pub struct PartUsage {
    pub part: Part,
    pub size: u64,
    pub items: Vec<Item>,
}

#[derive(Clone, Debug, Default)]
pub struct Storage {
    pub parts: Vec<Arc<PartUsage>>,
    pub measuring: bool,
}

impl Storage {
    pub fn measured(&self) -> u64 {
        self.parts.iter().map(|part| part.size).sum()
    }

    pub fn part(&self, part: Part) -> Option<&PartUsage> {
        self.parts.iter().find(|usage| usage.part == part).map(|usage| &**usage)
    }
}

#[derive(Default)]
struct Shared {
    latest: Option<Storage>,
    running: bool,
}

pub(super) struct Disk {
    shared: Arc<Mutex<Shared>>,
    wake: Wake,
    measured_at: Option<Instant>,
}

impl Disk {
    pub(super) fn new(wake: Wake) -> Self {
        Self { shared: Arc::default(), wake, measured_at: None }
    }

    pub(super) fn measure(&mut self) {
        let fresh = self.measured_at.is_some_and(|at| at.elapsed() < STALE_AFTER);
        let mut shared = lock(&self.shared);
        if fresh || shared.running {
            return;
        }
        shared.running = true;
        drop(shared);
        self.measured_at = Some(Instant::now());
        let (shared, wake) = (Arc::clone(&self.shared), Arc::clone(&self.wake));
        let spawned = thread::Builder::new().name("notchbase-disk".into()).spawn(move || run(&shared, &wake));
        if spawned.is_err() {
            lock(&self.shared).running = false;
        }
    }

    pub(super) fn take(&self) -> Option<Storage> {
        lock(&self.shared).latest.take()
    }
}

fn run(shared: &Mutex<Shared>, wake: &Wake) {
    let queue = Mutex::new(plan::roots());
    let total = Mutex::new(Tally::default());
    let publish = |storage: Storage| {
        let mut shared = lock(shared);
        shared.running = storage.measuring;
        shared.latest = Some(storage);
        drop(shared);
        wake();
    };
    publish(Storage { parts: Vec::new(), measuring: true });
    thread::scope(|scope| {
        for _ in 0..WORKERS {
            scope.spawn(|| {
                scan::be_gentle();
                while let Some(root) = { lock(&queue).pop() } {
                    let measured = scan::measure(&root);
                    let mut total = lock(&total);
                    total.merge(measured);
                    let storage = total.storage(true);
                    drop(total);
                    publish(storage);
                }
            });
        }
    });
    let mut storage = lock(&total).storage(false);
    scan::load_icons(&mut storage);
    publish(storage);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_part_has_a_title_and_a_place() {
        for (index, part) in Part::ALL.into_iter().enumerate() {
            assert_eq!(part as usize, index);
            assert!(!part.title().is_empty());
        }
        let apps = PartUsage { part: Part::Apps, size: 9, items: Vec::new() };
        let music = PartUsage { part: Part::Music, size: 4, items: Vec::new() };
        let storage = Storage { parts: vec![Arc::new(apps), Arc::new(music)], measuring: false };
        assert_eq!(storage.measured(), 13);
        assert_eq!(storage.part(Part::Music).map(|part| part.size), Some(4));
        assert!(storage.part(Part::Desktop).is_none());
    }
}
