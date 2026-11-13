//! The records the model keeps: one per file with its opens, edits, habits and skips, and one per folder with its heat.
//! `Seen` is what the index tells the model and `Used` is what the system says was opened.

use std::path::PathBuf;

use crate::find::context::{DAY, Daypart, Stamp};
use crate::find::key::Key;
use crate::find::weights::{EDIT_HALF_LIFE, EVICTION_OPEN_WORTH, HEAT_HALF_LIFE, OPEN_HALF_LIFE};

use super::decay::{decayed, since};
use super::table::Keyed;

pub const SLOTS: usize = 2;

pub mod state {
    pub const GONE: u8 = 1;
    pub const BENCHED: u8 = 2;
    pub const KNOWN: u8 = GONE | BENCHED;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Seen {
    Edited { path: PathBuf, at: Stamp, plain: bool },
    Added { path: PathBuf, file_id: u64, created: Stamp, plain: bool },
    Removed { path: PathBuf },
    Moved { from: PathBuf, to: PathBuf },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Used {
    pub path: PathBuf,
    pub last: Stamp,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FileRecord {
    pub key: Key,
    pub file_id: u64,
    pub last_open: Stamp,
    pub last_edit: Stamp,
    pub opens: f32,
    pub edits: f32,
    pub dayparts: [u8; Daypart::COUNT],
    pub apps: [u8; SLOTS],
    pub app_weight: [u8; SLOTS],
    pub skips: u8,
    pub flags: u8,
    pub day: u16,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FolderRecord {
    pub key: Key,
    pub heat: f32,
    pub opens: f32,
    pub last: Stamp,
}

pub fn day_of(at: Stamp) -> u16 {
    u16::try_from(at / DAY).unwrap_or(u16::MAX)
}

impl FileRecord {
    pub fn new(key: Key) -> Self {
        Self {
            key,
            file_id: 0,
            last_open: 0,
            last_edit: 0,
            opens: 0.0,
            edits: 0.0,
            dayparts: [0; Daypart::COUNT],
            apps: [0; SLOTS],
            app_weight: [0; SLOTS],
            skips: 0,
            flags: 0,
            day: 0,
        }
    }

    pub fn opened(&self) -> bool {
        self.last_open != 0
    }

    pub fn gone(&self) -> bool {
        self.flags & state::GONE != 0
    }

    pub fn opens_at(&self, now: Stamp) -> f32 {
        decayed(self.opens, since(now, self.last_open), OPEN_HALF_LIFE)
    }

    pub fn edits_at(&self, now: Stamp) -> f32 {
        decayed(self.edits, since(now, self.last_edit), EDIT_HALF_LIFE)
    }

    pub fn worth(&self, now: Stamp) -> f32 {
        EVICTION_OPEN_WORTH * self.opens_at(now) + self.edits_at(now)
    }

    pub fn open(&mut self, at: Stamp, weight: f32) {
        let at = at.max(self.last_open).max(1);
        self.opens = self.opens_at(at) + weight;
        self.last_open = at;
        self.flags &= !state::GONE;
    }

    pub fn edit(&mut self, at: Stamp) {
        let at = at.max(1);
        self.edits = self.edits_at(at) + 1.0;
        self.last_edit = at;
    }
}

impl FolderRecord {
    pub fn new(key: Key) -> Self {
        Self { key, heat: 0.0, opens: 0.0, last: 0 }
    }

    pub fn heat_at(&self, now: Stamp) -> f32 {
        decayed(self.heat, since(now, self.last), HEAT_HALF_LIFE)
    }

    pub fn opens_at(&self, now: Stamp) -> f32 {
        decayed(self.opens, since(now, self.last), OPEN_HALF_LIFE)
    }

    pub fn worth(&self, now: Stamp) -> f32 {
        self.heat_at(now) + self.opens_at(now)
    }

    pub fn warm(&mut self, at: Stamp, heat: f32, opens: f32) {
        let at = at.max(self.last);
        self.heat = self.heat_at(at) + heat;
        self.opens = self.opens_at(at) + opens;
        self.last = at;
    }
}

impl Keyed for FileRecord {
    fn key(&self) -> Key {
        self.key
    }

    fn rekey(&mut self, key: Key) {
        self.key = key;
    }
}

impl Keyed for FolderRecord {
    fn key(&self) -> Key {
        self.key
    }

    fn rekey(&mut self, key: Key) {
        self.key = key;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::find::context::HOUR;
    use crate::find::key::START;

    const NOON: Stamp = 1_704_110_400;
    const CLOSE: f32 = 1e-3;

    #[test]
    fn open_folds_in_the_decay() {
        let mut record = FileRecord::new(START);
        assert!(!record.opened());
        record.open(NOON, 1.0);
        record.open(NOON + OPEN_HALF_LIFE, 1.0);
        assert!((record.opens - 1.5).abs() < CLOSE);
        assert_eq!(record.last_open, NOON + OPEN_HALF_LIFE);
        assert!((record.opens_at(NOON + 2 * OPEN_HALF_LIFE) - 0.75).abs() < CLOSE);
    }

    #[test]
    fn older_open_still_counts() {
        let mut record = FileRecord::new(START);
        record.open(NOON, 1.0);
        record.open(NOON - HOUR, 1.0);
        assert_eq!((record.opens, record.last_open), (2.0, NOON));
    }

    #[test]
    fn opening_brings_record_back() {
        let mut record = FileRecord::new(START);
        record.flags = state::GONE | state::BENCHED;
        record.open(NOON, 1.0);
        assert!(!record.gone());
        assert_eq!(record.flags, state::BENCHED);
    }

    #[test]
    fn open_worth_four_edits() {
        let (mut opened, mut edited) = (FileRecord::new(START), FileRecord::new(START));
        opened.open(NOON, 1.0);
        (0..3).for_each(|_| edited.edit(NOON));
        assert!(opened.worth(NOON) > edited.worth(NOON));
        edited.edit(NOON);
        edited.edit(NOON);
        assert!(opened.worth(NOON) < edited.worth(NOON));
    }

    #[test]
    fn heat_cools_faster_than_opens() {
        let mut folder = FolderRecord::new(START);
        folder.warm(NOON, 4.0, 4.0);
        let later = NOON + HEAT_HALF_LIFE;
        assert!((folder.heat_at(later) - 2.0).abs() < CLOSE);
        assert!(folder.opens_at(later) > 3.9);
        folder.warm(later, 1.0, 0.0);
        assert!((folder.heat - 3.0).abs() < CLOSE);
        assert_eq!(folder.last, later);
    }

    #[test]
    fn day_number_fits_a_century() {
        assert_eq!(day_of(0), 0);
        assert_eq!(day_of(NOON), 19_723);
        assert_eq!(day_of(Stamp::MAX), 49_710);
    }
}
