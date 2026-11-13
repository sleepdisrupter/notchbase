//! Rows added since the last rebuild, found by their parent and name through a hash.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

use super::base::Dead;
use super::columns::Columns;
use super::{Entry, EntryId};

const NO_OLDER: u32 = u32::MAX;

#[derive(Default)]
pub(in crate::find) struct Overlay {
    pub columns: Columns,
    pub dead: Dead,
    named: HashMap<u32, u32>,
    newest: HashMap<EntryId, u32>,
    older: Vec<u32>,
}

impl Overlay {
    pub fn len(&self) -> usize {
        self.columns.len()
    }

    pub fn push(&mut self, parent: EntryId, entry: Entry<'_>) -> usize {
        let row = self.columns.push(parent, entry);
        self.named.insert(name_hash(parent, entry.name), row as u32);
        self.older.push(self.newest.insert(parent, row as u32).unwrap_or(NO_OLDER));
        row
    }

    pub fn find(&self, parent: EntryId, name: &str) -> Option<usize> {
        let is_it = |row: &usize| self.columns.parent[*row] == parent && self.columns.is_named(*row, name);
        let row = *self.named.get(&name_hash(parent, name))? as usize;
        if is_it(&row) {
            return (!self.dead.has(row)).then_some(row);
        }
        self.children(parent).find(|row| !self.dead.has(*row) && is_it(row))
    }

    #[cfg(test)]
    pub fn bytes(&self) -> usize {
        let slots = (self.named.capacity() + self.newest.capacity()) * (size_of::<(u32, u32)>() + 1);
        self.columns.bytes() + slots + self.older.capacity() * size_of::<u32>()
    }

    pub fn children(&self, parent: EntryId) -> impl Iterator<Item = usize> + '_ {
        let older = |row: &u32| self.older.get(*row as usize).copied().filter(|older| *older != NO_OLDER);
        std::iter::successors(self.newest.get(&parent).copied(), older).map(|row| row as usize)
    }
}

fn name_hash(parent: EntryId, name: &str) -> u32 {
    let mut hasher = DefaultHasher::new();
    (parent, name).hash(&mut hasher);
    hasher.finish() as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::find::model::columns::NO_PARENT;

    fn file(name: &str) -> Entry<'_> {
        Entry { name, modified: 1, created: 1, flags: 0 }
    }

    fn filled() -> Overlay {
        let mut overlay = Overlay::default();
        for (parent, name) in [(NO_PARENT, "/"), (0, "a.txt"), (0, "B.txt"), (7, "a.txt"), (0, "b.txt")] {
            overlay.push(parent, file(name));
        }
        overlay
    }

    #[test]
    fn row_found_by_parent_and_name() {
        let overlay = filled();
        assert_eq!(overlay.len(), 5);
        assert_eq!(overlay.find(NO_PARENT, "/"), Some(0));
        assert_eq!((overlay.find(0, "a.txt"), overlay.find(7, "a.txt")), (Some(1), Some(3)));
        assert_eq!((overlay.find(0, "B.txt"), overlay.find(0, "b.txt")), (Some(2), Some(4)));
        assert_eq!((overlay.find(0, "A.txt"), overlay.find(1, "a.txt"), overlay.find(0, "")), (None, None, None));
    }

    #[test]
    fn children_listed_newest_first() {
        let overlay = filled();
        assert_eq!(overlay.children(0).collect::<Vec<_>>(), [4, 2, 1]);
        assert_eq!(overlay.children(7).collect::<Vec<_>>(), [3]);
        assert_eq!(overlay.children(NO_PARENT).collect::<Vec<_>>(), [0]);
        assert_eq!(overlay.children(3).count(), 0);
    }

    #[test]
    fn buried_name_gone_until_added() {
        let mut overlay = filled();
        assert!(overlay.dead.mark(1));
        assert_eq!(overlay.find(0, "a.txt"), None);
        let again = overlay.push(0, file("a.txt"));
        assert_eq!((again, overlay.find(0, "a.txt")), (5, Some(5)));
        assert_eq!(overlay.find(7, "a.txt"), Some(3), "its namesake elsewhere never moved");
    }

    #[test]
    fn hash_twins_both_found() {
        let mut overlay = filled();
        let taken = name_hash(0, "a.txt");
        overlay.named.insert(taken, 4);
        assert_eq!(overlay.find(0, "a.txt"), Some(1), "the slot now holds another row");
        assert_eq!(overlay.find(0, "b.txt"), Some(4));
        overlay.dead.mark(4);
        assert_eq!(overlay.find(0, "a.txt"), Some(1));
        overlay.dead.mark(1);
        assert_eq!(overlay.find(0, "a.txt"), None);
    }
}
