//! What Find learns from use. `Usage` keeps the model of what you open, edit and pick, and saves it to usage.bin two
//! seconds after a pick and otherwise every ten minutes.

mod decay;
mod disk;
mod folded;
mod forgive;
mod habits;
mod heat;
mod learn;
mod model;
mod moves;
mod pairs;
mod prefix;
mod record;
mod table;
#[cfg(test)]
mod tests;

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use super::context::{Context, MINUTE, Stamp};
use super::key::Key;
use super::kind::Kind;
use super::shared::lock;
use super::store::frame;
use super::view::Pick;

pub(in crate::find) use decay::{faded, filled, since};
pub use model::Model;
pub use record::*;

const USAGE_SAVE_EVERY: Stamp = 10 * MINUTE;
const SAVED_AFTER_A_PICK: Stamp = 2;
const UNFINISHED: &str = ".tmp";

#[derive(Clone)]
pub struct Usage {
    model: Arc<Mutex<Model>>,
    file: Option<Arc<Path>>,
}

impl Usage {
    pub fn load(file: Option<&Path>, now: Stamp) -> Self {
        let stored = file.and_then(|file| frame::read(file, frame::USAGE));
        let learned = stored.as_ref().and_then(disk::decode);
        if let (Some(file), Some(_), None) = (file, &stored, &learned) {
            let _ = fs::remove_file(file);
        }
        let mut model = learned.unwrap_or_default();
        model.expire(now);
        model.book = model::Book { dirty: false, saved_at: now, picked_at: None };
        Self { model: Arc::new(Mutex::new(model)), file: file.map(Arc::from) }
    }

    pub fn forgotten(file: Option<&Path>, now: Stamp) -> Self {
        let usage = Self { file: file.map(Arc::from), ..Self::load(None, now) };
        usage.forget();
        usage
    }

    pub fn read<T>(&self, with: impl FnOnce(&Model) -> T) -> T {
        with(&self.model())
    }

    pub fn matching(&self, needles: &[&[u8]]) -> Vec<(Key, Box<str>)> {
        self.model().matching(needles)
    }

    pub fn picked(&self, pick: &Pick, kind: Kind, context: &Context) {
        self.model().picked(pick, kind, context);
    }

    pub fn left(&self, predicted: &[Key], picked: Option<Key>, now: Stamp) {
        self.model().passed_by(predicted, picked, now);
    }

    pub fn observe(&self, seen: &[Seen], now: Stamp) {
        let mut model = self.model();
        for seen in seen {
            match seen {
                Seen::Edited { path, at, plain } => model.edited(path, *at, *plain, now),
                Seen::Added { path, file_id, .. } => model.added(path, *file_id, now),
                Seen::Removed { path } => model.removed(path, now),
                Seen::Moved { from, to } => model.moved(from, to),
            }
        }
    }

    pub fn seed(&self, used: &[Used], now: Stamp) {
        let mut model = self.model();
        used.iter().for_each(|used| model.seeded(used, now));
    }

    pub fn purge(&self, keep: &dyn Fn(&Path) -> bool) {
        self.model().purge(keep);
    }

    pub fn forget(&self) {
        *self.model() = Model::default();
        for file in self.file.iter().flat_map(|file| [file.to_path_buf(), unfinished(file)]) {
            let _ = fs::remove_file(file);
        }
    }

    pub fn save_due(&self) -> Option<Stamp> {
        self.file.as_ref()?;
        let book = self.model().book;
        let every = book.saved_at.saturating_add(USAGE_SAVE_EVERY);
        let after_a_pick = book.picked_at.map(|at| at.saturating_add(SAVED_AFTER_A_PICK));
        book.dirty.then(|| after_a_pick.map_or(every, |at| at.min(every)))
    }

    pub fn save_if_due(&self, now: Stamp) -> io::Result<bool> {
        let Some(file) = &self.file else { return Ok(false) };
        let frame = {
            let mut model = self.model();
            let book = model.book;
            let after_a_pick = book.picked_at.is_some_and(|at| since(now, at) >= SAVED_AFTER_A_PICK);
            if !book.dirty || !(after_a_pick || since(now, book.saved_at) >= USAGE_SAVE_EVERY) {
                return Ok(false);
            }
            model.expire(now);
            model.book = model::Book { dirty: false, saved_at: now, picked_at: None };
            disk::encode(&model, now)
        };
        let written = frame::write(file, &frame);
        if written.is_err() {
            self.model().book.dirty = true;
        }
        written.map(|()| true)
    }

    fn model(&self) -> MutexGuard<'_, Model> {
        lock(&self.model)
    }
}

fn unfinished(file: &Path) -> PathBuf {
    let mut beside = file.as_os_str().to_owned();
    beside.push(UNFINISHED);
    PathBuf::from(beside)
}
