//! Typed memories: what you picked after typing what. The text is lowered, loses its spaces and separators and is cut
//! to 12 bytes. A shorter text recalls what was picked for a longer one, and a longer text recalls less.

use std::cell::RefCell;

use crate::find::context::Stamp;
use crate::find::key::Key;
use crate::find::weights::{
    PASSED_OVER, PREFIX_BYTES, PREFIX_EXTENDED, PREFIX_FULL_WEIGHT, PREFIX_HALF_LIFE, PREFIXES,
};

use super::decay::{decayed, share, since};

const SEPARATORS: &[u8] = b"_-";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Typed {
    bytes: [u8; PREFIX_BYTES],
    len: u8,
}

impl Typed {
    pub fn of(folded: &[u8]) -> Self {
        let mut typed = Self::default();
        let spoken = folded.iter().filter(|byte| !byte.is_ascii_whitespace() && !SEPARATORS.contains(byte));
        for byte in spoken.take(PREFIX_BYTES) {
            typed.bytes[usize::from(typed.len)] = byte.to_ascii_lowercase();
            typed.len += 1;
        }
        typed
    }

    pub fn text(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len)]
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Memory {
    pub typed: Typed,
    pub key: Key,
    pub weight: f32,
    pub last: Stamp,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Recalled {
    key: Key,
    direct: f32,
    extended: f32,
}

#[derive(Default)]
struct Recall {
    typed: Typed,
    now: Stamp,
    files: Vec<Recalled>,
    total: f32,
}

#[derive(Default)]
pub struct Prefixes {
    rows: Vec<Memory>,
    recall: RefCell<Recall>,
}

impl Prefixes {
    pub fn from_rows(mut rows: Vec<Memory>) -> Self {
        rows.retain(|row| !row.typed.is_empty());
        rows.sort_by_key(|row| (row.typed, row.key));
        rows.dedup_by(|next, kept| {
            let same = (next.typed, next.key) == (kept.typed, kept.key);
            if same {
                kept.take_in(next);
            }
            same
        });
        rows.truncate(PREFIXES);
        Self { rows, recall: RefCell::default() }
    }

    pub fn rows(&self) -> &[Memory] {
        &self.rows
    }

    pub fn learn(&mut self, typed: Typed, key: Key, weight: f32, now: Stamp) {
        if typed.is_empty() {
            return;
        }
        match self.rows.binary_search_by_key(&(typed, key), |row| (row.typed, row.key)) {
            Ok(at) => {
                let row = &mut self.rows[at];
                row.weight = row.weight_at(now) + weight;
                row.last = now.max(row.last);
            }
            Err(at) => {
                self.rows.insert(at, Memory { typed, key, weight, last: now });
                self.keep_within(PREFIXES, at, now);
            }
        }
        self.recall.take();
    }

    pub fn strength(&self, typed: Typed, key: Key, now: Stamp) -> f32 {
        if typed.is_empty() || self.rows.is_empty() {
            return 0.0;
        }
        self.recalled(typed, now, |recall| {
            let Ok(at) = recall.files.binary_search_by_key(&key, |file| file.key) else { return 0.0 };
            let file = recall.files[at];
            let weight = file.direct + file.extended;
            let blend = share(file.direct + PREFIX_EXTENDED * file.extended, weight);
            share(weight, recall.total) * (weight / PREFIX_FULL_WEIGHT).min(1.0) * blend
        })
    }

    pub fn pass_over(&mut self, typed: Typed, picked: Key, now: Stamp) {
        let strongest = |recall: &Recall| {
            let others = recall.files.iter().filter(|file| file.key != picked);
            others.max_by(|a, b| (a.direct + a.extended).total_cmp(&(b.direct + b.extended))).map(|file| file.key)
        };
        if typed.is_empty() {
            return;
        }
        let Some(passed) = self.recalled(typed, now, strongest) else { return };
        let told = |row: &&mut Memory| row.key == passed && related(row.typed, typed).is_some();
        self.rows.iter_mut().filter(told).for_each(|row| row.weight *= PASSED_OVER);
        self.recall.take();
    }

    pub fn forget(&mut self, gone: impl Fn(Key) -> bool) {
        self.rows.retain(|row| !gone(row.key));
        self.recall.take();
    }

    pub fn rekey(&mut self, renamed: impl Fn(Key) -> Option<Key>) {
        for row in &mut self.rows {
            row.key = renamed(row.key).unwrap_or(row.key);
        }
        *self = Self::from_rows(std::mem::take(&mut self.rows));
    }

    fn keep_within(&mut self, most: usize, newest: usize, now: Stamp) {
        if self.rows.len() <= most {
            return;
        }
        let others = self.rows.iter().enumerate().filter(|(at, _)| *at != newest);
        let weakest = others.min_by(|a, b| a.1.weight_at(now).total_cmp(&b.1.weight_at(now))).map(|(at, _)| at);
        if let Some(weakest) = weakest {
            self.rows.remove(weakest);
        }
    }

    fn recalled<T>(&self, typed: Typed, now: Stamp, read: impl FnOnce(&Recall) -> T) -> T {
        let mut recall = self.recall.borrow_mut();
        if recall.typed != typed || recall.now != now {
            *recall = self.recall_of(typed, now);
        }
        read(&recall)
    }

    fn recall_of(&self, typed: Typed, now: Stamp) -> Recall {
        let from = self.rows.partition_point(|row| row.typed < typed);
        let longer = self.rows[from..].iter().take_while(|row| row.typed.text().starts_with(typed.text()));
        let shorter = (1..typed.text().len()).flat_map(|cut| {
            let stored = Typed::of(&typed.text()[..cut]);
            let from = self.rows.partition_point(|row| row.typed < stored);
            self.rows[from..].iter().take_while(move |row| row.typed == stored)
        });
        let mut files: Vec<Recalled> = longer
            .map(|row| Recalled { key: row.key, direct: row.weight_at(now), extended: 0.0 })
            .chain(shorter.map(|row| Recalled { key: row.key, direct: 0.0, extended: row.weight_at(now) }))
            .collect();
        files.sort_by_key(|file| file.key);
        files.dedup_by(|next, kept| {
            let same = next.key == kept.key;
            if same {
                kept.direct += next.direct;
                kept.extended += next.extended;
            }
            same
        });
        let total = files.iter().map(|file| file.direct + file.extended).sum();
        Recall { typed, now, files, total }
    }
}

impl Memory {
    pub fn weight_at(&self, now: Stamp) -> f32 {
        decayed(self.weight, since(now, self.last), PREFIX_HALF_LIFE)
    }

    fn take_in(&mut self, same: &Memory) {
        let last = self.last.max(same.last);
        (self.weight, self.last) = (self.weight_at(last) + same.weight_at(last), last);
    }
}

fn related(stored: Typed, typed: Typed) -> Option<bool> {
    if stored.text().starts_with(typed.text()) {
        Some(true)
    } else {
        typed.text().starts_with(stored.text()).then_some(false)
    }
}
