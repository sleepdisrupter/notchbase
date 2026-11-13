//! `Live`, the index the threads share: the base, the overlay and the burials, with a revision for every change.

use std::sync::atomic::{AtomicU64, Ordering};

use super::base::Base;
use super::columns::{Columns, NO_PARENT};
use super::overlay::Overlay;
use super::{Arena, Entry, EntryId, Rebuilt, Room, flag, merge, overlay_limit};

const DEAD_SHARE: usize = 8;
const KEPT_BY_REFLAG: u8 = flag::IDENTITY | flag::EXOTIC;

#[derive(Default)]
pub struct Live {
    pub(super) base: Base,
    pub(super) overlay: Overlay,
    pub(super) roots: Vec<EntryId>,
    unsettled: Vec<EntryId>,
    buried: usize,
    generation: u64,
    revision: AtomicU64,
}

impl Live {
    pub fn empty() -> Self {
        Self::default()
    }

    pub(in crate::find) fn over(base: Base) -> Self {
        let roots = (0..base.children(NO_PARENT).end as EntryId).collect();
        Self { base, roots, ..Self::default() }
    }

    pub(in crate::find) fn base(&self) -> &Base {
        &self.base
    }

    #[cfg(test)]
    pub(in crate::find) fn added_bytes(&self) -> usize {
        self.overlay.bytes()
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn revision(&self) -> u64 {
        self.revision.load(Ordering::Acquire)
    }

    pub fn len(&self) -> usize {
        self.base.len() + self.overlay.len()
    }

    pub fn alive(&self) -> usize {
        self.len() - self.buried
    }

    pub fn arenas(&self) -> [Arena<'_>; 2] {
        let added = Arena { bytes: &self.overlay.columns.arena, first: self.base.len() as EntryId };
        [Arena { bytes: &self.base.columns.arena, first: 0 }, added]
    }

    pub fn id_at(&self, arena: usize, offset: usize) -> EntryId {
        match arena {
            0 => self.base.columns.row_at(offset) as EntryId,
            _ => (self.base.len() + self.overlay.columns.row_at(offset)) as EntryId,
        }
    }

    pub fn folded(&self, id: EntryId) -> &[u8] {
        let (columns, row) = self.row(id);
        columns.folded(row)
    }

    pub fn name_into(&self, id: EntryId, out: &mut String) {
        let (columns, row) = self.row(id);
        columns.name_into(row, out);
    }

    pub fn parent(&self, id: EntryId) -> Option<EntryId> {
        let (columns, row) = self.row(id);
        Some(columns.parent[row]).filter(|parent| *parent != NO_PARENT)
    }

    pub fn modified(&self, id: EntryId) -> u32 {
        let (columns, row) = self.row(id);
        columns.modified(row)
    }

    pub fn created(&self, id: EntryId) -> u32 {
        let (columns, row) = self.row(id);
        columns.created[row]
    }

    pub fn flags(&self, id: EntryId) -> u8 {
        let (columns, row) = self.row(id);
        columns.flags(row)
    }

    pub fn is_dead(&self, id: EntryId) -> bool {
        match (id as usize).checked_sub(self.base.len()) {
            None => self.base.dead.has(id as usize),
            Some(row) => self.overlay.dead.has(row),
        }
    }

    pub fn children_into(&self, dir: EntryId, out: &mut Vec<EntryId>) {
        out.clear();
        let first = self.base.len();
        let settled = self.base.children(dir).filter(|row| (dir as usize) < first && !self.base.dead.has(*row));
        out.extend(settled.map(|row| row as EntryId));
        let settled = out.len();
        let added = self.overlay.children(dir).filter(|row| !self.overlay.dead.has(*row));
        out.extend(added.map(|row| (first + row) as EntryId));
        if out.len() > settled {
            out.sort_by(|left, right| {
                let ((ours, row), (theirs, their)) = (self.row(*left), self.row(*right));
                ours.order(row, theirs, their).then(left.cmp(right))
            });
        }
    }

    pub fn add(&mut self, parent: Option<EntryId>, entry: Entry<'_>) -> EntryId {
        let id = self.len() as EntryId;
        self.overlay.push(parent.unwrap_or(NO_PARENT), entry);
        match parent {
            None => self.roots.push(id),
            Some(parent) if self.is_dead(parent) => self.unsettled.push(parent),
            Some(_) => {}
        }
        self.revision.fetch_add(1, Ordering::AcqRel);
        id
    }

    pub fn touch(&self, id: EntryId, modified: u32) {
        let (columns, row) = self.row(id);
        columns.modified[row].store(modified, Ordering::Relaxed);
        self.revision.fetch_add(1, Ordering::AcqRel);
    }

    pub fn reflag(&self, id: EntryId, flags: u8) {
        let (columns, row) = self.row(id);
        let kept = columns.flags(row) & KEPT_BY_REFLAG;
        columns.flags[row].store(kept | (flags & !KEPT_BY_REFLAG), Ordering::Relaxed);
        self.revision.fetch_add(1, Ordering::AcqRel);
    }

    pub fn bury(&mut self, id: EntryId) {
        if self.mark_dead(id) {
            self.unsettled.push(id);
            self.revision.fetch_add(1, Ordering::AcqRel);
        }
    }

    pub fn settle(&mut self) {
        let first = self.base.len();
        let mut below = Vec::new();
        while let Some(dir) = self.unsettled.pop() {
            below.clear();
            below.extend(self.base.children(dir).map(|row| row as EntryId));
            below.extend(self.overlay.children(dir).map(|row| (first + row) as EntryId));
            for id in below.drain(..) {
                if self.mark_dead(id) {
                    self.unsettled.push(id);
                    self.revision.fetch_add(1, Ordering::AcqRel);
                }
            }
        }
    }

    pub fn wants_rebuild(&self) -> bool {
        self.overlay.len() >= overlay_limit(self.base.len()) || self.buried * DEAD_SHARE > self.len()
    }

    pub fn rebuilt(&self, room: &Room) -> Rebuilt {
        Rebuilt { base: merge::rebuild(&self.base, &self.overlay, room, self.alive()) }
    }

    pub fn adopt(&mut self, rebuilt: Rebuilt) {
        let (generation, revision) = (self.generation + 1, AtomicU64::new(self.revision() + 1));
        *self = Live { generation, revision, ..Live::over(rebuilt.base) };
    }

    pub fn settled(&self) -> bool {
        self.overlay.len() == 0 && self.buried == 0
    }

    fn row(&self, id: EntryId) -> (&Columns, usize) {
        match (id as usize).checked_sub(self.base.len()) {
            None => (&self.base.columns, id as usize),
            Some(row) => (&self.overlay.columns, row),
        }
    }

    fn mark_dead(&mut self, id: EntryId) -> bool {
        let newly = match (id as usize).checked_sub(self.base.len()) {
            None => self.base.dead.mark(id as usize),
            Some(row) => self.overlay.dead.mark(row),
        };
        self.buried += usize::from(newly);
        newly
    }
}
