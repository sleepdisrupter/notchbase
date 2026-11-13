//! `Shelf`, the list of items the notch shows. It skips paths it already holds, keeps the list in shelf.json, hands
//! work to the worker thread and folds what comes back into the items.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use egui::ColorImage;

use super::ops::{Outputs, Source};
use super::worker::{Busy, Done, Info, Worker};
use super::{Item, ItemId, Notice, Op, Origin, path_key, store};
use crate::poll::Wake;

const STORE_FILE: &str = "shelf.json";
const FALLBACK_OUTPUT_DIR: &str = "Shelf";

pub struct Shelf {
    items: Vec<Item>,
    path_keys: HashMap<ItemId, String>,
    next_id: u64,
    notice: Option<Notice>,
    store_file: Option<PathBuf>,
    worker: Worker,
    progress_at_last_sync: Option<Busy>,
}

impl Shelf {
    pub fn new(wake: Wake) -> Self {
        if cfg!(test) {
            return Self::with_locations(wake, None, None);
        }
        let data_dir = crate::platform::data_dir();
        let store_file = data_dir.as_ref().map(|dir| dir.join(STORE_FILE));
        let fallback_output_dir = data_dir.map(|dir| dir.join(FALLBACK_OUTPUT_DIR));
        Self::with_locations(wake, store_file, fallback_output_dir)
    }

    pub(super) fn with_locations(
        wake: Wake,
        store_file: Option<PathBuf>,
        fallback_output_dir: Option<PathBuf>,
    ) -> Self {
        let mut shelf = Self {
            items: Vec::new(),
            path_keys: HashMap::new(),
            next_id: 1,
            notice: None,
            store_file,
            worker: Worker::new(wake, Outputs::new(fallback_output_dir)),
            progress_at_last_sync: None,
        };
        shelf.restore_saved_items();
        shelf
    }

    pub fn items(&self) -> &[Item] {
        &self.items
    }

    pub fn add(&mut self, paths: Vec<PathBuf>) -> usize {
        self.add_as(paths, Origin::Dropped)
    }

    fn add_as(&mut self, paths: Vec<PathBuf>, origin: Origin) -> usize {
        let added = self.insert(paths, origin).len();
        if added > 0 {
            self.save();
        }
        added
    }

    pub fn remove(&mut self, id: ItemId) {
        let Some(index) = self.items.iter().position(|item| item.id == id) else { return };
        self.items.remove(index);
        self.forget(id);
        self.save();
    }

    pub fn clear(&mut self) {
        if self.items.is_empty() {
            return;
        }
        let ids: Vec<ItemId> = self.items.drain(..).map(|item| item.id).collect();
        for id in ids {
            self.forget(id);
        }
        self.save();
    }

    pub fn run(&mut self, op: Op, ids: &[ItemId]) {
        if self.worker.busy().is_some() {
            return;
        }
        let (items, sources) = self.sources(ids);
        if sources.is_empty() {
            return;
        }
        if !self.worker.start(op, items, sources) {
            self.notice = Some(Notice::failed("Couldn't start the operation"));
        }
    }

    pub fn busy(&self) -> Option<Busy> {
        self.worker.busy()
    }

    pub fn cancel(&self) {
        self.worker.cancel();
    }

    pub fn sync(&mut self) -> bool {
        let (finished, progress) = self.worker.take();
        let mut changed = progress != self.progress_at_last_sync;
        self.progress_at_last_sync = progress;
        for done in finished {
            changed |= self.apply(done);
        }
        changed
    }

    pub fn take_notice(&mut self) -> Option<Notice> {
        self.notice.take()
    }

    pub fn prune_missing(&mut self) -> bool {
        let vanished: Vec<ItemId> = self.items.iter().filter(|item| !item.path.exists()).map(|item| item.id).collect();
        if vanished.is_empty() {
            return false;
        }
        self.items.retain(|item| !vanished.contains(&item.id));
        for id in vanished {
            self.forget(id);
        }
        self.save();
        true
    }

    fn restore_saved_items(&mut self) {
        let Some(file) = &self.store_file else { return };
        let saved = store::load(file);
        let saved_count = saved.len();
        if self.insert(saved, Origin::Restored).len() != saved_count {
            self.save();
        }
    }

    fn insert(&mut self, paths: Vec<PathBuf>, origin: Origin) -> Vec<ItemId> {
        let now = Instant::now();
        paths.into_iter().filter_map(|path| self.insert_one(path, now, origin)).collect()
    }

    fn insert_one(&mut self, path: PathBuf, now: Instant, origin: Origin) -> Option<ItemId> {
        let path = std::path::absolute(&path).unwrap_or(path);
        let metadata = std::fs::metadata(&path).ok()?;
        let key = path_key::of(&path);
        if self.path_keys.values().any(|existing| *existing == key) {
            return None;
        }
        let id = ItemId(self.next_id);
        self.next_id += 1;
        let item = Item::new(id, path, &metadata, now, origin);
        self.worker.watch(id, item.path.clone(), item.kind);
        self.path_keys.insert(id, key);
        self.items.push(item);
        Some(id)
    }

    fn forget(&mut self, id: ItemId) {
        self.path_keys.remove(&id);
        self.worker.forget(id);
    }

    fn sources(&self, ids: &[ItemId]) -> (Vec<ItemId>, Vec<Source>) {
        let mut seen = HashSet::new();
        ids.iter()
            .filter(|id| seen.insert(**id))
            .filter_map(|id| self.items.iter().find(|item| item.id == *id))
            .map(|item| (item.id, Source { path: item.path.clone(), kind: item.kind }))
            .unzip()
    }

    fn item_mut(&mut self, id: ItemId) -> Option<&mut Item> {
        self.items.iter_mut().find(|item| item.id == id)
    }

    fn apply(&mut self, done: Done) -> bool {
        match done {
            Done::Info(info) => self.apply_info(info),
            Done::Thumb { id, thumb } => self.apply_thumb(id, thumb),
            Done::Op(outcome) => {
                self.add_as(outcome.outputs, Origin::Produced);
                self.notice = Some(outcome.notice);
                true
            }
        }
    }

    fn apply_info(&mut self, info: Info) -> bool {
        let Some(item) = self.item_mut(info.id) else { return false };
        let before = (item.bytes, item.dims, item.pages, item.seconds);
        item.bytes = info.bytes.or(item.bytes);
        item.dims = info.dims;
        item.pages = info.pages;
        item.seconds = info.seconds;
        before != (item.bytes, item.dims, item.pages, item.seconds)
    }

    fn apply_thumb(&mut self, id: ItemId, thumb: Arc<ColorImage>) -> bool {
        let Some(item) = self.item_mut(id) else { return false };
        item.thumb = Some(thumb);
        item.thumb_rev += 1;
        true
    }

    fn save(&self) {
        let Some(file) = &self.store_file else { return };
        if let Err(err) = store::save(file, self.items.iter().map(|item| item.path.as_path())) {
            eprintln!("notchbase: could not save the shelf: {err}");
        }
    }
}
