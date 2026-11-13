//! Holds changes until they are due and merges repeats. A pending subtree or relist absorbs what happens below it, and
//! more than 32 touched children of one folder become a single relist.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::Instant;

use super::{Change, Event};

pub(super) const RELIST_AT: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Held {
    pub id: u64,
    pub since: Instant,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Slot(Change);

impl Hash for Slot {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(&self.0).hash(state);
        match &self.0 {
            Change::Moved { from, to } => (from, to).hash(state),
            Change::Touched(path)
            | Change::Relist(path)
            | Change::Subtree(path)
            | Change::Overflow(path)
            | Change::Everything(path) => path.hash(state),
        }
    }
}

#[derive(Default)]
pub(super) struct Lane {
    held: HashMap<Slot, Held>,
    subtrees: HashSet<PathBuf>,
    relists: HashSet<PathBuf>,
    touched: HashMap<PathBuf, usize>,
}

impl Lane {
    pub(super) fn hold(&mut self, change: Change, id: u64, now: Instant) {
        if let Some(cover) = self.cover(&change) {
            self.merge(cover, Held { id, since: now });
            return;
        }
        let parent = touched_parent(&change).map(Path::to_path_buf);
        let fresh = !self.held.contains_key(&Slot(change.clone()));
        self.index(&change);
        self.merge(change, Held { id, since: now });
        let Some(parent) = parent.filter(|_| fresh) else { return };
        let count = self.touched.entry(parent.clone()).or_default();
        *count += 1;
        if *count > RELIST_AT {
            self.collapse(parent);
        }
    }

    pub(super) fn take(&mut self, ready: impl Fn(&Held) -> bool) -> Vec<Event> {
        let mut out = Vec::new();
        self.held.retain(|Slot(change), held| {
            let leaving = ready(held);
            if leaving {
                out.push(Event { change: change.clone(), id: held.id });
            }
            !leaving
        });
        for event in &out {
            self.unindex(&event.change);
        }
        out
    }

    pub(super) fn oldest(&self) -> Option<u64> {
        self.held.values().map(|held| held.id).min()
    }

    pub(super) fn is_empty(&self) -> bool {
        self.held.is_empty()
    }

    fn cover(&self, change: &Change) -> Option<Change> {
        let (path, strict) = match change {
            Change::Touched(path) => (path, true),
            Change::Relist(path) | Change::Subtree(path) => (path, false),
            _ => return None,
        };
        let above = path.ancestors().skip(usize::from(strict));
        let subtree = above.filter(|dir| !dir.as_os_str().is_empty()).find(|dir| self.subtrees.contains(*dir));
        if let Some(dir) = subtree.filter(|dir| change != &Change::Subtree(dir.to_path_buf())) {
            return Some(Change::Subtree(dir.to_path_buf()));
        }
        let parent = touched_parent(change)?;
        self.relists.contains(parent).then(|| Change::Relist(parent.to_path_buf()))
    }

    fn merge(&mut self, change: Change, arriving: Held) {
        let held = self.held.entry(Slot(change)).or_insert(arriving);
        held.id = held.id.min(arriving.id);
        held.since = held.since.min(arriving.since);
    }

    fn collapse(&mut self, parent: PathBuf) {
        let mut absorbed: Option<Held> = None;
        self.held.retain(|Slot(change), held| {
            let child = touched_parent(change) == Some(parent.as_path());
            if child {
                let kept = absorbed.get_or_insert(*held);
                (kept.id, kept.since) = (kept.id.min(held.id), kept.since.min(held.since));
            }
            !child
        });
        self.touched.remove(&parent);
        let relist = Change::Relist(parent);
        self.index(&relist);
        if let Some(held) = absorbed {
            self.merge(relist, held);
        }
    }

    fn index(&mut self, change: &Change) {
        match change {
            Change::Subtree(path) => self.subtrees.insert(path.clone()),
            Change::Relist(path) => self.relists.insert(path.clone()),
            _ => false,
        };
    }

    fn unindex(&mut self, change: &Change) {
        match change {
            Change::Subtree(path) => {
                self.subtrees.remove(path);
            }
            Change::Relist(path) => {
                self.relists.remove(path);
            }
            Change::Touched(path) => {
                if let Some(parent) = path.parent()
                    && let Some(count) = self.touched.get_mut(parent)
                {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        self.touched.remove(parent);
                    }
                }
            }
            _ => {}
        }
    }
}

fn touched_parent(change: &Change) -> Option<&Path> {
    match change {
        Change::Touched(path) => path.parent(),
        _ => None,
    }
}
