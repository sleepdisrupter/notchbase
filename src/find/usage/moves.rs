//! Keeping what was learned when files move. A removed file is hidden and kept for 30 days, and when one with the same
//! file id turns up elsewhere its record, memories and sequences follow it there.

use std::collections::HashMap;
use std::path::Path;

use crate::find::context::{DAY, Stamp};
use crate::find::key::{Key, key_of};
use crate::find::weights::{GONE_KEPT, MOVE_WINDOW};

use super::decay::since;
use super::model::Model;
use super::record::{day_of, state};

const GONE_KEPT_DAYS: u16 = (GONE_KEPT / DAY) as u16;
const ARRIVALS_REMEMBERED: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Removed {
    key: Key,
    file_id: u64,
    at: Stamp,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Arrived {
    path: Box<str>,
    file_id: u64,
    at: Stamp,
}

impl Model {
    pub(super) fn removed(&mut self, path: &Path, now: Stamp) {
        self.forget_old_sightings(now);
        let below = |stored: &str| Path::new(stored).starts_with(path);
        let mut renamed = Vec::new();
        for key in self.files.keys_where(|record, stored| !record.gone() && below(stored)) {
            let Some(record) = self.files.get_mut(key) else { continue };
            let file_id = record.file_id;
            let same_file = |arrived: &Arrived| file_id != 0 && arrived.file_id == file_id;
            if let Some(at) = self.arrived.iter().position(same_file) {
                renamed.push((key, self.arrived.swap_remove(at).path.into_string()));
                continue;
            }
            record.flags |= state::GONE;
            record.day = day_of(now);
            if file_id != 0 {
                self.removed.push(Removed { key, file_id, at: now });
            }
            self.book.dirty = true;
        }
        for key in self.folders.keys_where(|_, stored| below(stored)) {
            self.folders.take(key);
        }
        self.rekey(renamed);
        self.previous = self.previous.filter(|(key, _)| self.known(*key).is_some());
    }

    pub(super) fn added(&mut self, path: &Path, file_id: u64, now: Stamp) {
        self.forget_old_sightings(now);
        let key = key_of(path);
        let left = self.removed.iter().position(|removed| file_id != 0 && removed.file_id == file_id);
        if let Some(old) = left.map(|at| self.removed.swap_remove(at).key).filter(|old| *old != key) {
            self.rekey(vec![(old, path.to_string_lossy().into_owned())]);
        }
        let Some(record) = self.files.get_mut(key) else { return self.remember_arrival(path, file_id, now) };
        let identified = if file_id == 0 { record.file_id } else { file_id };
        if record.gone() || record.file_id != identified {
            record.flags &= !state::GONE;
            record.file_id = identified;
            self.book.dirty = true;
        }
    }

    fn remember_arrival(&mut self, path: &Path, file_id: u64, now: Stamp) {
        if file_id == 0 || self.files.is_empty() {
            return;
        }
        if self.arrived.len() >= ARRIVALS_REMEMBERED {
            self.arrived.remove(0);
        }
        self.arrived.push(Arrived { path: path.to_string_lossy().into(), file_id, at: now });
    }

    fn forget_old_sightings(&mut self, now: Stamp) {
        self.removed.retain(|removed| since(now, removed.at) <= MOVE_WINDOW);
        self.arrived.retain(|arrived| since(now, arrived.at) <= MOVE_WINDOW);
    }

    pub(super) fn moved(&mut self, from: &Path, to: &Path) {
        if from.as_os_str().is_empty() || to.as_os_str().is_empty() || from == to {
            return;
        }
        let landed = |stored: &str| {
            let below = Path::new(stored).strip_prefix(from).ok()?;
            Some(if below.as_os_str().is_empty() { to.to_path_buf() } else { to.join(below) })
        };
        let files = self.files.each().filter_map(|(record, stored)| Some((record.key, landed(stored)?)));
        let folders = self.folders.each().filter_map(|(record, stored)| Some((record.key, landed(stored)?)));
        let moves: Vec<(Key, String)> =
            files.chain(folders).map(|(key, path)| (key, path.to_string_lossy().into_owned())).collect();
        self.rekey(moves);
    }

    pub(super) fn purge(&mut self, keep: &dyn Fn(&Path) -> bool) {
        let excluded = self.files.keys_where(|_, stored| !keep(Path::new(stored)));
        self.drop_files(&excluded);
        for key in self.folders.keys_where(|_, stored| !keep(Path::new(stored))) {
            self.folders.take(key);
            self.book.dirty = true;
        }
    }

    pub(super) fn expire(&mut self, now: Stamp) {
        let today = day_of(now);
        let kept_long_enough =
            self.files.keys_where(|record, _| record.gone() && today.saturating_sub(record.day) > GONE_KEPT_DAYS);
        self.drop_files(&kept_long_enough);
    }

    fn rekey(&mut self, moves: Vec<(Key, String)>) {
        let mut renamed = HashMap::new();
        for (old, path) in moves {
            let new = key_of(Path::new(&path));
            let moved = self.files.move_to(old, new, &path) | self.folders.move_to(old, new, &path);
            if let Some(record) = self.files.get_mut(new).filter(|_| moved) {
                record.flags &= !state::GONE;
            }
            if moved && old != new {
                renamed.insert(old, new);
            }
        }
        if renamed.is_empty() {
            return;
        }
        self.prefixes.rekey(|key| renamed.get(&key).copied());
        self.pairs.rekey(|key| renamed.get(&key).copied());
        self.previous = self.previous.map(|(key, at)| (renamed.get(&key).copied().unwrap_or(key), at));
        self.book.dirty = true;
    }
}
