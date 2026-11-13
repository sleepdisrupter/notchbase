//! The settled part of the index: rows ordered so each folder's children form one run, and a mark for each buried row.

use std::ops::Range;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering::Relaxed;

use super::EntryId;
use super::columns::Columns;

const WORD: usize = u64::BITS as usize;

#[derive(Default)]
pub(in crate::find) struct Dead {
    words: Vec<AtomicU64>,
}

impl Dead {
    pub fn has(&self, row: usize) -> bool {
        self.words.get(row / WORD).is_some_and(|word| word.load(Relaxed) >> (row % WORD) & 1 != 0)
    }

    pub fn mark(&mut self, row: usize) -> bool {
        if self.words.len() <= row / WORD {
            self.words.resize_with(row / WORD + 1, AtomicU64::default);
        }
        let bit = 1 << (row % WORD);
        self.words[row / WORD].fetch_or(bit, Relaxed) & bit == 0
    }
}

impl PartialEq for Dead {
    fn eq(&self, other: &Self) -> bool {
        let marked = |dead: &Dead| -> Vec<usize> {
            let rows = 0..dead.words.len() * WORD;
            rows.filter(|row| dead.has(*row)).collect()
        };
        marked(self) == marked(other)
    }
}

#[derive(Default, PartialEq)]
pub(in crate::find) struct Base {
    pub columns: Columns,
    pub dead: Dead,
}

pub(in crate::find) fn rank(parent: EntryId) -> u32 {
    parent.wrapping_add(1)
}

impl Base {
    pub fn over(columns: Columns) -> Self {
        Self { columns, dead: Dead::default() }
    }

    pub fn len(&self) -> usize {
        self.columns.len()
    }

    pub fn children(&self, dir: EntryId) -> Range<usize> {
        let parents = &self.columns.parent;
        let from = parents.partition_point(|parent| rank(*parent) < rank(dir));
        from..from + parents[from..].partition_point(|parent| *parent == dir)
    }

    pub fn child(&self, dir: EntryId, folded: &[u8], name: &str) -> Option<usize> {
        let mut rows = self.children(dir);
        while rows.start < rows.end {
            let middle = rows.start + (rows.end - rows.start) / 2;
            if self.columns.folded(middle) < folded {
                rows.start = middle + 1;
            } else {
                rows.end = middle;
            }
        }
        let alike = |row: &usize| self.columns.parent[*row] == dir && self.columns.folded(*row) == folded;
        let mut run = (rows.start..self.len()).take_while(alike);
        run.find(|row| !self.dead.has(*row) && self.columns.is_named(*row, name))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering::Relaxed;

    use super::*;
    use crate::find::model::columns::NO_PARENT;
    use crate::find::model::{Entry, flag};

    const ROWS: [(EntryId, &str); 9] = [
        (NO_PARENT, "/"),
        (NO_PARENT, "/Volumes/Work"),
        (0, "Applications"),
        (0, "Makefile"),
        (0, "makefile"),
        (0, "R\u{e9}sum\u{e9}.pdf"),
        (0, "resume.pdf"),
        (1, "notes.txt"),
        (2, "Safari.app"),
    ];

    fn base() -> Base {
        let mut columns = Columns::default();
        for (parent, name) in ROWS {
            columns.push(parent, Entry { name, modified: 7, created: 3, flags: flag::DIR });
        }
        Base::over(columns)
    }

    fn found(base: &Base, dir: EntryId, name: &str) -> Option<usize> {
        base.child(dir, &crate::find::model::fold_query(name), name)
    }

    #[test]
    fn children_form_one_run() {
        let base = base();
        assert_eq!(base.children(NO_PARENT), 0..2);
        assert_eq!(base.children(0), 2..7);
        assert_eq!(base.children(1), 7..8);
        assert_eq!(base.children(2), 8..9);
        assert_eq!(base.children(3), 9..9, "a file has none");
        assert_eq!(base.children(8), 9..9);
        assert!(Base::default().children(NO_PARENT).is_empty());
    }

    #[test]
    fn child_found_by_exact_name() {
        let mut base = base();
        for (row, (parent, name)) in ROWS.iter().enumerate() {
            assert_eq!(found(&base, *parent, name), Some(row), "{name}");
        }
        assert_eq!(found(&base, 0, "MAKEFILE"), None);
        assert_eq!(found(&base, 0, "Resume.pdf"), None);
        assert_eq!(found(&base, 0, "r\u{e9}sum\u{e9}.pdf"), None);
        assert_eq!(found(&base, 1, "Makefile"), None, "another folder's child");
        assert_eq!(found(&base, 0, "zebra"), None);
        assert_eq!(found(&base, 0, "aardvark"), None);
        assert!(base.dead.mark(3) && !base.dead.mark(3), "a row dies once");
        assert_eq!((found(&base, 0, "Makefile"), found(&base, 0, "makefile")), (None, Some(4)));
    }

    #[test]
    fn bases_equal_when_all_match() {
        let (mut left, right) = (base(), base());
        assert!(left == right);
        left.columns.modified[4].store(8, Relaxed);
        assert!(left != right, "a newer save");
        left.columns.modified[4].store(7, Relaxed);
        left.columns.flags[4].store(flag::DIR | flag::HIDDEN, Relaxed);
        assert!(left != right, "another flag");
        left.columns.flags[4].store(flag::DIR, Relaxed);
        assert!(left == right);
        left.dead.mark(200);
        assert!(left != right, "a burial");
        assert!(left.dead.has(200) && !left.dead.has(199) && !right.dead.has(200));
    }
}
