//! A table of records found by the key of their path. Every change to the paths bumps the edition, so the folded copy
//! knows when to build again.

use std::collections::HashMap;

use crate::find::key::Key;

pub trait Keyed {
    fn key(&self) -> Key;
    fn rekey(&mut self, key: Key);
}

pub struct Table<R> {
    rows: Vec<R>,
    paths: Vec<Box<str>>,
    slots: HashMap<Key, u32>,
    edition: u64,
}

impl<R> Default for Table<R> {
    fn default() -> Self {
        Self { rows: Vec::new(), paths: Vec::new(), slots: HashMap::new(), edition: 0 }
    }
}

impl<R: Keyed> Table<R> {
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn edition(&self) -> u64 {
        self.edition
    }

    pub fn rows(&self) -> &[R] {
        &self.rows
    }

    pub fn each(&self) -> impl Iterator<Item = (&R, &str)> {
        self.rows.iter().zip(self.paths.iter().map(|path| &**path))
    }

    pub fn at(&self, slot: usize) -> Option<(&R, &str)> {
        Some((self.rows.get(slot)?, self.paths.get(slot)?))
    }

    pub fn get(&self, key: Key) -> Option<&R> {
        self.slots.get(&key).and_then(|slot| self.rows.get(*slot as usize))
    }

    pub fn get_mut(&mut self, key: Key) -> Option<&mut R> {
        self.slots.get(&key).and_then(|slot| self.rows.get_mut(*slot as usize))
    }

    pub fn path(&self, key: Key) -> Option<&str> {
        self.slots.get(&key).and_then(|slot| self.paths.get(*slot as usize)).map(|path| &**path)
    }

    pub fn put(&mut self, row: R, path: &str) {
        self.edition += 1;
        match self.slots.get(&row.key()) {
            Some(slot) => {
                self.rows[*slot as usize] = row;
                self.paths[*slot as usize] = path.into();
            }
            None => {
                self.slots.insert(row.key(), self.rows.len() as u32);
                self.rows.push(row);
                self.paths.push(path.into());
            }
        }
    }

    pub fn take(&mut self, key: Key) -> Option<(R, Box<str>)> {
        let slot = self.slots.remove(&key)? as usize;
        self.edition += 1;
        let taken = (self.rows.swap_remove(slot), self.paths.swap_remove(slot));
        if let Some(moved) = self.rows.get(slot) {
            self.slots.insert(moved.key(), slot as u32);
        }
        Some(taken)
    }

    pub fn move_to(&mut self, old: Key, new: Key, path: &str) -> bool {
        let Some((mut row, _)) = self.take(old) else { return false };
        row.rekey(new);
        self.put(row, path);
        true
    }

    pub fn keys_where(&self, wanted: impl Fn(&R, &str) -> bool) -> Vec<Key> {
        self.each().filter(|(row, path)| wanted(row, path)).map(|(row, _)| row.key()).collect()
    }

    #[cfg(test)]
    pub fn clear(&mut self) {
        self.edition += 1;
        self.rows.clear();
        self.paths.clear();
        self.slots.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::find::usage::record::FolderRecord;

    fn folder(key: u64) -> FolderRecord {
        FolderRecord::new(Key(key))
    }

    fn filled() -> Table<FolderRecord> {
        let mut table = Table::default();
        for (key, path) in [(1, "/a"), (2, "/b"), (3, "/c")] {
            table.put(folder(key), path);
        }
        table
    }

    #[test]
    fn row_found_by_key() {
        let table = filled();
        assert_eq!(table.len(), 3);
        assert_eq!(table.path(Key(2)), Some("/b"));
        assert_eq!(table.get(Key(3)).map(|row| row.key), Some(Key(3)));
        assert!(table.get(Key(9)).is_none() && table.path(Key(9)).is_none());
    }

    #[test]
    fn taking_keeps_others_findable() {
        let mut table = filled();
        assert_eq!(table.take(Key(1)).map(|(_, path)| path), Some("/a".into()));
        assert!(table.take(Key(1)).is_none());
        assert_eq!((table.len(), table.path(Key(3)), table.path(Key(2))), (2, Some("/c"), Some("/b")));
        assert_eq!(table.take(Key(3)).map(|(row, _)| row.key), Some(Key(3)));
        assert_eq!(table.each().map(|(row, path)| (row.key.0, path)).collect::<Vec<_>>(), [(2, "/b")]);
    }

    #[test]
    fn putting_again_replaces() {
        let mut table = filled();
        let mut warmer = folder(2);
        warmer.heat = 5.0;
        table.put(warmer, "/moved");
        assert_eq!(table.len(), 3);
        assert_eq!((table.path(Key(2)), table.get(Key(2)).map(|row| row.heat)), (Some("/moved"), Some(5.0)));
    }

    #[test]
    fn path_changes_bump_edition() {
        let mut table = filled();
        let before = table.edition();
        if let Some(row) = table.get_mut(Key(1)) {
            row.heat = 2.0;
        }
        assert_eq!(table.edition(), before, "a counter is not a path");
        table.take(Key(1));
        let after_take = table.edition();
        table.put(folder(7), "/g");
        table.clear();
        assert!(before < after_take && after_take < table.edition() && table.is_empty());
    }

    #[test]
    fn moved_row_takes_new_key() {
        let mut table = filled();
        assert!(table.move_to(Key(1), Key(3), "/c"));
        assert!(!table.move_to(Key(1), Key(8), "/h"));
        assert_eq!((table.len(), table.get(Key(1)).is_none(), table.path(Key(3))), (2, true, Some("/c")));
        assert_eq!(table.get(Key(3)).map(|row| row.key), Some(Key(3)));
    }

    #[test]
    fn rows_picked_by_content() {
        let table = filled();
        assert_eq!(table.keys_where(|row, path| row.key.0 > 1 && path != "/c"), [Key(2)]);
    }
}
