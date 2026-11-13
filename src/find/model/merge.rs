//! The rebuild: merges the overlay into a new base and drops what was buried.

use std::cmp::Ordering;
use std::ops::Range;

use super::base::{Base, rank};
use super::columns::{Columns, NO_PARENT};
use super::evict::Valve;
use super::overlay::Overlay;
use super::{EntryId, Room};

const ROW_BITS: u32 = u32::BITS;

struct Added {
    rows: Vec<u64>,
    under_added: usize,
    starts: Vec<u32>,
}

struct Walk<'a> {
    base: &'a Base,
    overlay: &'a Overlay,
    added: &'a Added,
    valve: Valve,
    settled_at: usize,
    added_at: usize,
    next: Columns,
    sources: Vec<EntryId>,
}

pub(super) fn rebuild(base: &Base, overlay: &Overlay, room: &Room, alive: usize) -> Base {
    let added = Added::sorted(base.len(), overlay);
    let rows = alive.min(room.cap);
    let name_bytes = base.columns.arena.len() + overlay.columns.arena.len();
    let next = Columns::with_room(rows, name_bytes);
    let (valve, sources) = (Valve::of(room), Vec::with_capacity(rows));
    let mut walk = Walk { base, overlay, added: &added, valve, settled_at: 0, added_at: 0, next, sources };
    walk.emit_children(NO_PARENT, NO_PARENT);
    let roots = walk.sources.len();
    let mut at = 0;
    while let Some(old) = walk.sources.get(at).copied() {
        if at >= roots || walk.valve.opens(old) {
            walk.emit_children(old, at as EntryId);
        }
        at += 1;
    }
    walk.next.shrink();
    Base::over(walk.next)
}

fn parent_rank(row: u64) -> usize {
    (row >> ROW_BITS) as usize
}

fn local(row: u64) -> usize {
    (row as u32) as usize
}

impl Added {
    fn sorted(first: usize, overlay: &Overlay) -> Self {
        let columns = &overlay.columns;
        let alive = (0..overlay.len()).filter(|row| !overlay.dead.has(*row));
        let mut rows: Vec<u64> =
            alive.map(|row| u64::from(rank(columns.parent[row])) << ROW_BITS | row as u64).collect();
        rows.sort_unstable();
        for siblings in rows.chunk_by_mut(|left, right| parent_rank(*left) == parent_rank(*right)) {
            siblings.sort_by(|left, right| columns.order(local(*left), columns, local(*right)));
        }
        let under_added = rows.partition_point(|row| parent_rank(*row) <= first);
        let mut starts = vec![0u32; overlay.len() + 1];
        for row in &rows[under_added..] {
            if let Some(count) = starts.get_mut(parent_rank(*row) - first - 1) {
                *count += 1;
            }
        }
        let mut before = 0;
        for start in &mut starts {
            before += std::mem::replace(start, before);
        }
        Self { rows, under_added, starts }
    }

    fn below_added(&self, parent: usize) -> Range<usize> {
        let at = |row: usize| self.under_added + self.starts.get(row).copied().unwrap_or(0) as usize;
        at(parent)..at(parent + 1).min(self.rows.len())
    }
}

impl<'a> Walk<'a> {
    fn emit_children(&mut self, old: EntryId, new: EntryId) {
        let (base, overlay): (&'a Base, &'a Overlay) = (self.base, self.overlay);
        let first = base.len();
        let (mut settled, mut added) = match (old as usize).checked_sub(first).filter(|_| old != NO_PARENT) {
            Some(row) => (0..0, self.added.below_added(row)),
            None => (self.settled_run(old), self.added_run(old)),
        };
        loop {
            let from_base = match (settled.is_empty(), added.is_empty()) {
                (true, true) => break,
                (false, true) => true,
                (true, false) => false,
                (false, false) => {
                    let theirs = local(self.added.rows[added.start]);
                    base.columns.order(settled.start, &overlay.columns, theirs) != Ordering::Greater
                }
            };
            if from_base {
                let row = settled.start;
                settled.start += 1;
                if !base.dead.has(row) {
                    self.emit(&base.columns, row, row, new);
                }
            } else {
                let row = local(self.added.rows[added.start]);
                added.start += 1;
                self.emit(&overlay.columns, row, first + row, new);
            }
        }
    }

    fn emit(&mut self, from: &Columns, row: usize, old: usize, new: EntryId) {
        if new == NO_PARENT || self.valve.keeps(from.flags(row)) {
            self.next.carry(from, row, new);
            self.sources.push(old as EntryId);
        }
    }

    fn settled_run(&mut self, parent: EntryId) -> Range<usize> {
        let parents = &self.base.columns.parent;
        let skipped = parents[self.settled_at..].iter().take_while(|above| rank(**above) < rank(parent)).count();
        let from = self.settled_at + skipped;
        self.settled_at = from + parents[from..].iter().take_while(|above| **above == parent).count();
        from..self.settled_at
    }

    fn added_run(&mut self, parent: EntryId) -> Range<usize> {
        let rows = &self.added.rows[..self.added.under_added];
        let wanted = rank(parent) as usize;
        let skipped = rows[self.added_at..].iter().take_while(|row| parent_rank(**row) < wanted).count();
        let from = self.added_at + skipped;
        self.added_at = from + rows[from..].iter().take_while(|row| parent_rank(**row) == wanted).count();
        from..self.added_at
    }
}
