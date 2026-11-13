//! The index's rows stored column by column: names, parents, stamps and flags.

use std::cmp::Ordering;
use std::ops::Range;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::atomic::{AtomicU8, AtomicU32};

use super::exotic::Exotics;
use super::fold::fold_into;
use super::{Entry, EntryId, flag};

pub(in crate::find) const NO_PARENT: EntryId = EntryId::MAX;
pub(in crate::find) const END_OF_NAME: u8 = 0;
const WORD: usize = u64::BITS as usize;

pub(in crate::find) struct Columns {
    pub parent: Vec<EntryId>,
    pub name_at: Vec<u32>,
    pub modified: Vec<AtomicU32>,
    pub created: Vec<u32>,
    pub flags: Vec<AtomicU8>,
    pub arena: Vec<u8>,
    pub upper: Vec<u64>,
    pub exotic: Exotics,
}

impl Default for Columns {
    fn default() -> Self {
        Self::with_room(0, 0)
    }
}

impl Columns {
    pub fn with_room(rows: usize, name_bytes: usize) -> Self {
        let mut arena = Vec::with_capacity(name_bytes + 1);
        arena.push(END_OF_NAME);
        Self {
            parent: Vec::with_capacity(rows),
            name_at: Vec::with_capacity(rows),
            modified: Vec::with_capacity(rows),
            created: Vec::with_capacity(rows),
            flags: Vec::with_capacity(rows),
            arena,
            upper: vec![0; (name_bytes + 1).div_ceil(WORD)],
            exotic: Exotics::default(),
        }
    }

    pub fn len(&self) -> usize {
        self.parent.len()
    }

    pub fn push(&mut self, parent: EntryId, entry: Entry<'_>) -> usize {
        let (row, at) = (self.len(), self.arena.len());
        let exotic = fold_into(entry.name, &mut self.arena);
        self.end_name();
        if exotic {
            self.exotic.push(row as u32, entry.name);
        } else {
            let capitals = |letters: &[u8]| {
                letters.iter().rev().fold(0, |bits, byte| bits << 1 | u64::from(byte.is_ascii_uppercase()))
            };
            entry
                .name
                .as_bytes()
                .chunks(WORD)
                .enumerate()
                .for_each(|(nth, letters)| self.put(at + nth * WORD, capitals(letters)));
        }
        let flags = if exotic { entry.flags | flag::EXOTIC } else { entry.flags & !flag::EXOTIC };
        self.finish_row(parent, at, (entry.modified, entry.created, flags));
        row
    }

    pub fn carry(&mut self, from: &Columns, row: usize, parent: EntryId) {
        let (span, at, flags) = (from.span(row), self.arena.len(), from.flags(row));
        self.arena.extend_from_slice(from.folded(row));
        self.end_name();
        for done in (0..span.len()).step_by(WORD) {
            self.put(at + done, from.bits(span.start + done, (span.len() - done).min(WORD)));
        }
        if flags & flag::EXOTIC != 0 {
            self.exotic.push(self.len() as u32, from.exotic.name(row as u32).unwrap_or_default());
        }
        self.finish_row(parent, at, (from.modified(row), from.created[row], flags));
    }

    pub fn modified(&self, row: usize) -> u32 {
        self.modified[row].load(Relaxed)
    }

    pub fn flags(&self, row: usize) -> u8 {
        self.flags[row].load(Relaxed)
    }

    pub fn span(&self, row: usize) -> Range<usize> {
        let from = self.name_at[row] as usize;
        let next = self.name_at.get(row + 1).map_or(self.arena.len(), |next| *next as usize);
        from..next.saturating_sub(1).max(from)
    }

    pub fn folded(&self, row: usize) -> &[u8] {
        self.arena.get(self.span(row)).unwrap_or_default()
    }

    pub fn row_at(&self, offset: usize) -> usize {
        self.name_at.partition_point(|at| *at as usize <= offset).saturating_sub(1)
    }

    pub fn name_into(&self, row: usize, out: &mut String) {
        out.clear();
        if self.flags(row) & flag::EXOTIC != 0 {
            return out.push_str(self.exotic.name(row as u32).unwrap_or_default());
        }
        let span = self.span(row);
        out.push_str(&String::from_utf8_lossy(self.folded(row)));
        for done in (0..span.len()).step_by(WORD) {
            let mut capitals = self.bits(span.start + done, (span.len() - done).min(WORD));
            while capitals != 0 {
                let at = done + capitals.trailing_zeros() as usize;
                out.get_mut(at..=at).into_iter().for_each(str::make_ascii_uppercase);
                capitals &= capitals - 1;
            }
        }
    }

    pub fn is_named(&self, row: usize, name: &str) -> bool {
        if self.flags(row) & flag::EXOTIC != 0 {
            return self.exotic.name(row as u32) == Some(name);
        }
        let (span, folded) = (self.span(row), self.folded(row));
        let same = |(nth, (ours, theirs)): (usize, (&u8, u8))| {
            *ours == theirs.to_ascii_lowercase()
                && self.bits(span.start + nth, 1) == u64::from(theirs.is_ascii_uppercase())
        };
        folded.len() == name.len() && folded.iter().zip(name.bytes()).enumerate().all(same)
    }

    pub fn order(&self, row: usize, other: &Columns, their: usize) -> Ordering {
        self.folded(row).cmp(other.folded(their)).then_with(|| self.original(row).cmp(&other.original(their)))
    }

    pub fn shrink(&mut self) {
        self.parent.shrink_to_fit();
        self.name_at.shrink_to_fit();
        self.modified.shrink_to_fit();
        self.created.shrink_to_fit();
        self.flags.shrink_to_fit();
        self.arena.shrink_to_fit();
        self.upper.truncate(self.arena.len().div_ceil(WORD));
        self.upper.shrink_to_fit();
        self.exotic.shrink();
    }

    #[cfg(test)]
    pub fn bytes(&self) -> usize {
        let stamps =
            self.parent.capacity() + self.name_at.capacity() + self.modified.capacity() + self.created.capacity();
        let names = self.arena.capacity() + self.upper.capacity() * size_of::<u64>() + self.exotic.bytes();
        stamps * size_of::<u32>() + self.flags.capacity() + names
    }

    fn original(&self, row: usize) -> String {
        let mut name = String::new();
        self.name_into(row, &mut name);
        name
    }

    fn end_name(&mut self) {
        self.arena.push(END_OF_NAME);
        let words = self.arena.len().div_ceil(WORD);
        if words > self.upper.len() {
            self.upper.resize(words, 0);
        }
    }

    fn finish_row(&mut self, parent: EntryId, at: usize, (modified, created, flags): (u32, u32, u8)) {
        self.parent.push(parent);
        self.name_at.push(at as u32);
        self.modified.push(AtomicU32::new(modified));
        self.created.push(created);
        self.flags.push(AtomicU8::new(flags));
    }

    fn bits(&self, at: usize, count: usize) -> u64 {
        let (word, shift) = (at / WORD, at % WORD);
        let read = |word: usize| self.upper.get(word).copied().unwrap_or(0);
        let spilled = if shift == 0 { 0 } else { read(word + 1) << (WORD - shift) };
        let wanted = if count >= WORD { u64::MAX } else { (1 << count) - 1 };
        (read(word) >> shift | spilled) & wanted
    }

    fn put(&mut self, at: usize, bits: u64) {
        let (word, shift) = (at / WORD, at % WORD);
        let spilled = if shift == 0 { 0 } else { bits >> (WORD - shift) };
        for (word, bits) in [(word, bits << shift), (word + 1, spilled)] {
            if let Some(held) = self.upper.get_mut(word).filter(|_| bits != 0) {
                *held |= bits;
            }
        }
    }
}

impl PartialEq for Columns {
    fn eq(&self, other: &Self) -> bool {
        let stamps = |columns: &Columns| columns.modified.iter().map(|stamp| stamp.load(Relaxed)).collect::<Vec<u32>>();
        let flags = |columns: &Columns| columns.flags.iter().map(|flags| flags.load(Relaxed)).collect::<Vec<u8>>();
        let words =
            |columns: &Columns| columns.upper[..columns.arena.len().div_ceil(WORD).min(columns.upper.len())].to_vec();
        self.parent == other.parent
            && self.name_at == other.name_at
            && self.created == other.created
            && self.arena == other.arena
            && self.exotic == other.exotic
            && stamps(self) == stamps(other)
            && flags(self) == flags(other)
            && words(self) == words(other)
    }
}
