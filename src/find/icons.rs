//! The icon thread. Icons are drawn by file type, never from the file itself, with one for folders and one per app.

use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread;

use egui::ColorImage;

use crate::config::FindConfig;
use crate::platform::{self, KnownDir};
use crate::poll::Wake;

use super::scan::Plan;
use super::view::{FindRow, Kind};
use super::{ICONS_THREAD, os};

const ICON_PX: u32 = 64;
const PROBE: &str = "probe";
const LONGEST_ENDING: usize = 16;
const MOST_KEPT: usize = 512;

type Load = Box<dyn Fn(&Path, u32) -> Option<ColorImage> + Send>;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Look {
    Type(String),
    Folder,
    App(PathBuf),
}

impl Look {
    fn of(row: &FindRow) -> Look {
        match (row.app || row.kind == Kind::App, row.folder) {
            (true, _) => Look::App(row.path.to_path_buf()),
            (_, true) => Look::Folder,
            _ => Look::Type(ending(&row.name)),
        }
    }
}

fn ending(name: &str) -> String {
    let ending = name.rsplit_once('.').filter(|(stem, _)| !stem.is_empty()).map_or("", |(_, ending)| ending);
    let plain = ending.len() <= LONGEST_ENDING && ending.bytes().all(|byte| byte.is_ascii_alphanumeric());
    if plain { ending.to_ascii_lowercase() } else { String::new() }
}

#[derive(Default)]
struct Store {
    wanted: Vec<(Look, bool)>,
    kept: HashMap<Look, Option<Arc<ColorImage>>>,
    stopped: bool,
}

#[derive(Default)]
struct Inner {
    store: Mutex<Store>,
    asked: Condvar,
    version: AtomicU64,
}

impl Inner {
    fn store(&self) -> MutexGuard<'_, Store> {
        self.store.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

pub struct Icons {
    inner: Option<Arc<Inner>>,
}

impl Icons {
    pub fn start(wake: Wake, enabled: bool, scratch: Option<PathBuf>) -> Self {
        if !enabled {
            return Self { inner: None };
        }
        let home = platform::known_dir(KnownDir::Home);
        let artist = Artist { scratch, home, plan: None, load: Box::new(crate::filedrag::icon) };
        Self::run(wake, artist)
    }

    fn run(wake: Wake, mut artist: Artist) -> Self {
        let inner = Arc::new(Inner::default());
        let shared = inner.clone();
        let spawned = thread::Builder::new().name(ICONS_THREAD.into()).spawn(move || artist.serve(&shared, &wake));
        Self { inner: spawned.is_ok().then_some(inner) }
    }

    pub fn want(&self, rows: &[FindRow], limited: bool) {
        let Some(inner) = &self.inner else { return };
        let mut store = inner.store();
        let before = store.wanted.len();
        for look in rows.iter().map(Look::of) {
            if !store.kept.contains_key(&look) && !store.wanted.iter().any(|(wanted, _)| *wanted == look) {
                store.wanted.push((look, limited));
            }
        }
        if store.wanted.len() > before {
            inner.asked.notify_one();
        }
    }

    pub fn of(&self, row: &FindRow) -> Option<Arc<ColorImage>> {
        self.inner.as_ref().and_then(|inner| inner.store().kept.get(&Look::of(row)).cloned().flatten())
    }

    pub fn version(&self) -> u64 {
        self.inner.as_ref().map_or(0, |inner| inner.version.load(Ordering::Acquire))
    }
}

impl Drop for Icons {
    fn drop(&mut self) {
        if let Some(inner) = &self.inner {
            inner.store().stopped = true;
            inner.asked.notify_one();
        }
    }
}

struct Refused;

struct Probe(PathBuf);

impl Drop for Probe {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

struct Artist {
    scratch: Option<PathBuf>,
    home: Option<PathBuf>,
    plan: Option<Plan>,
    load: Load,
}

impl Artist {
    fn serve(&mut self, inner: &Inner, wake: &Wake) {
        while let Some(batch) = next_batch(inner) {
            let mut drawn = false;
            for (look, limited) in batch {
                let Ok(image) = catch_unwind(AssertUnwindSafe(|| self.draw(&look, limited))) else { continue };
                let Ok(image) = image else { continue };
                keep(&mut inner.store(), look, image.map(Arc::new));
                drawn = true;
            }
            if drawn {
                inner.version.fetch_add(1, Ordering::AcqRel);
                wake();
            }
        }
    }

    fn draw(&mut self, look: &Look, limited: bool) -> Result<Option<ColorImage>, Refused> {
        match look {
            Look::Type(ending) => Ok(self.probe(ending)),
            Look::Folder => Ok(self.scratch().and_then(|scratch| (self.load)(&scratch, ICON_PX))),
            Look::App(path) if limited && !self.plan().wanted(path) => Err(Refused),
            Look::App(path) => Ok((self.load)(path, ICON_PX)),
        }
    }

    fn scratch(&self) -> Option<PathBuf> {
        let scratch = self.scratch.clone()?;
        fs::create_dir_all(&scratch).ok()?;
        Some(scratch)
    }

    fn probe(&self, ending: &str) -> Option<ColorImage> {
        let name = if ending.is_empty() { PROBE.to_owned() } else { format!("{PROBE}.{ending}") };
        let file = self.scratch()?.join(name);
        private_file(&file).ok()?;
        let probe = Probe(file);
        (self.load)(&probe.0, ICON_PX)
    }

    fn plan(&mut self) -> &Plan {
        let home = self.home.as_deref();
        self.plan.get_or_insert_with(|| Plan::new(&FindConfig::default(), &os::roots(true), home, None, false))
    }
}

fn next_batch(inner: &Inner) -> Option<Vec<(Look, bool)>> {
    let mut store = inner.store();
    while store.wanted.is_empty() && !store.stopped {
        store = inner.asked.wait(store).unwrap_or_else(PoisonError::into_inner);
    }
    (!store.stopped).then(|| std::mem::take(&mut store.wanted))
}

fn keep(store: &mut Store, look: Look, image: Option<Arc<ColorImage>>) {
    if store.kept.len() >= MOST_KEPT {
        store.kept.retain(|kept, _| !matches!(kept, Look::App(_)));
    }
    store.kept.insert(look, image);
}

fn private_file(path: &Path) -> std::io::Result<fs::File> {
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)
}

#[cfg(test)]
mod tests;
