use std::cmp::Ordering;

use crate::find::model::columns::{Columns, NO_PARENT};
use crate::find::model::{Entry, flag, fold_query};

const LONG: &str =
    "A Very Long Name That Runs Past One Word Of Capital Bits And Keeps Going Until It Has Crossed Two.TXT";
const NAMES: [&str; 12] = [
    "readme.md",
    "README.md",
    "ReadMe.MD",
    "R\u{e9}sum\u{e9}.PDF",
    "Re\u{301}sume\u{301}.PDF",
    "\u{41e}\u{422}\u{427}\u{401}\u{422}.docx",
    "\u{43e}\u{442}\u{447}\u{435}\u{442} Final.DOCX",
    "\u{6587}\u{4ef6} Copy.TXT",
    "Stra\u{df}e",
    "",
    LONG,
    "Z",
];

fn entry(name: &str) -> Entry<'_> {
    Entry { name, modified: 20, created: 10, flags: flag::HIDDEN | flag::EXOTIC }
}

fn filled() -> Columns {
    let mut columns = Columns::default();
    NAMES.iter().for_each(|name| assert_eq!(columns.push(NO_PARENT, entry(name)), columns.len() - 1));
    columns
}

fn name_of(columns: &Columns, row: usize) -> String {
    let mut name = String::from("left over");
    columns.name_into(row, &mut name);
    name
}

#[test]
fn names_come_back_exactly() {
    let columns = filled();
    for (row, name) in NAMES.iter().enumerate() {
        assert_eq!(name_of(&columns, row), *name);
        assert_eq!(columns.folded(row), fold_query(name), "{name}");
        assert!(columns.is_named(row, name), "{name}");
    }
    assert_eq!(columns.arena.iter().filter(|byte| **byte == 0).count(), NAMES.len() + 1);
}

#[test]
fn only_exotic_names_kept_aside() {
    let columns = filled();
    let aside: Vec<usize> = (0..NAMES.len()).filter(|row| columns.flags(*row) & flag::EXOTIC != 0).collect();
    assert_eq!(aside, [3, 4, 5, 8], "accents, combining marks, capitals outside ASCII, a sharp s");
    assert_eq!(columns.exotic.len(), aside.len());
    assert!(aside.iter().all(|row| columns.exotic.name(*row as u32) == Some(NAMES[*row])));
    assert!((0..NAMES.len()).all(|row| columns.flags(row) & !flag::EXOTIC == flag::HIDDEN));
}

#[test]
fn name_is_only_itself() {
    let columns = filled();
    assert!(!columns.is_named(0, "README.md") && !columns.is_named(1, "readme.md"));
    assert!(!columns.is_named(1, "README.mD") && !columns.is_named(1, "README.m") && !columns.is_named(1, ""));
    assert!(!columns.is_named(3, "Re\u{301}sume\u{301}.PDF") && !columns.is_named(4, "R\u{e9}sum\u{e9}.PDF"));
    assert!(
        !columns.is_named(3, "Resume.PDF") && !columns.is_named(6, "\u{43e}\u{442}\u{447}\u{435}\u{442} final.DOCX")
    );
    assert!(!columns.is_named(10, &LONG.to_lowercase()) && !columns.is_named(9, "a"));
}

#[test]
fn any_byte_leads_to_row() {
    let columns = filled();
    for (row, name) in NAMES.iter().enumerate() {
        let span = columns.span(row);
        assert_eq!(span.len(), fold_query(name).len());
        assert!(span.clone().all(|offset| columns.row_at(offset) == row), "{name}");
    }
    assert_eq!(Columns::default().row_at(0), 0);
}

#[test]
fn names_order_by_fold_first() {
    let columns = filled();
    assert_eq!(columns.order(1, &columns, 0), Ordering::Less, "capitals sort before small letters");
    assert_eq!(columns.order(1, &columns, 2), Ordering::Less);
    assert_eq!(columns.order(0, &columns, 0), Ordering::Equal);
    assert_eq!(columns.order(0, &columns, 3), Ordering::Less, "readme.md is before resume.pdf");
    assert_ne!(columns.order(3, &columns, 4), Ordering::Equal, "one fold, two spellings");
    assert_eq!(columns.order(3, &columns, 4), columns.order(4, &columns, 3).reverse());
    assert_eq!(columns.order(9, &columns, 11), Ordering::Less, "the empty name is first");
}

#[test]
fn carried_row_keeps_everything() {
    let (from, mut into) = (filled(), Columns::with_room(NAMES.len(), 8));
    into.push(NO_PARENT, entry("first, so nothing lines up"));
    for row in (0..NAMES.len()).rev() {
        from.modified[row].store(row as u32, std::sync::atomic::Ordering::Relaxed);
        into.carry(&from, row, 0);
    }
    for (row, name) in NAMES.iter().rev().enumerate() {
        let (new, old) = (row + 1, NAMES.len() - 1 - row);
        assert_eq!(name_of(&into, new), *name);
        assert_eq!((into.parent[new], into.modified(new), into.created[new]), (0, old as u32, 10));
        assert_eq!((into.flags(new), into.folded(new)), (from.flags(old), from.folded(old)));
    }
    assert_eq!(into.exotic.len(), from.exotic.len());
    into.shrink();
    assert_eq!(name_of(&into, 2), LONG);
    assert!(into != from);
}
