//! Checks a loaded index before it is used: names end where they should, parents come before their children,
//! siblings are in order and every exotic name has its row. Anything off and the file is refused.

use std::cmp::Ordering;

use crate::find::model::base::{Base, rank};
use crate::find::model::columns::{Columns, END_OF_NAME, NO_PARENT};
use crate::find::model::flag;

pub(super) fn name_offsets(arena: &[u8], entries: usize) -> Option<Vec<u32>> {
    let mut ends = memchr::memchr_iter(END_OF_NAME, arena);
    let mut starts = Vec::with_capacity(entries.min(arena.len()));
    starts.extend(ends.by_ref().take(entries).map(|end| end as u32 + 1));
    let opens = arena.first() == Some(&END_OF_NAME);
    let closes = ends.next().is_some_and(|last| last + 1 == arena.len()) && ends.next().is_none();
    (opens && closes && starts.len() == entries).then_some(starts)
}

pub(super) fn sound(base: &Base) -> bool {
    let columns = &base.columns;
    parents_come_first(&columns.parent) && siblings_are_sorted(columns) && exotic_names_are_all_there(columns)
}

fn parents_come_first(parents: &[u32]) -> bool {
    let above = parents.iter().enumerate().all(|(row, parent)| *parent == NO_PARENT || (*parent as usize) < row);
    above && parents.windows(2).all(|pair| rank(pair[0]) <= rank(pair[1]))
}

fn siblings_are_sorted(columns: &Columns) -> bool {
    let in_order = |row: usize| {
        columns.parent[row - 1] != columns.parent[row] || columns.order(row - 1, columns, row) != Ordering::Greater
    };
    (1..columns.len()).all(in_order)
}

fn exotic_names_are_all_there(columns: &Columns) -> bool {
    let is_exotic = |row: usize| columns.flags.get(row).is_some_and(|_| columns.flags(row) & flag::EXOTIC != 0);
    let mut last = None;
    let ascending = columns.exotic.each().all(|(row, _)| last.replace(row).is_none_or(|before| before < row));
    let flagged = (0..columns.len()).filter(|row| is_exotic(*row)).count();
    ascending && flagged == columns.exotic.len() && columns.exotic.each().all(|(row, _)| is_exotic(row as usize))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_follow_one_another() {
        assert_eq!(name_offsets(b"\0", 0), Some(Vec::new()));
        assert_eq!(name_offsets(b"\0a\0\0bcd\0", 3), Some(vec![1, 3, 4]));
        assert_eq!(name_offsets(b"\0a\0\0bcd\0", 2), None, "a name nobody counted");
        assert_eq!(name_offsets(b"\0a\0\0bcd\0", 4), None, "a name that is not there");
        assert_eq!(name_offsets(b"\0a\0bcd", 2), None, "the last name never ends");
        assert_eq!(name_offsets(b"a\0b\0", 1), None, "the arena does not open with an end of name");
        assert_eq!(name_offsets(b"", 0), None);
        assert_eq!(name_offsets(b"\0", usize::MAX), None, "a count far beyond the bytes reserves nothing");
    }
}
