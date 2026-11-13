use egui::pos2;

use super::{rect, sheet, shows};
use crate::annotate::Sheet;
use crate::annotate::lettering::Lettering;
use crate::annotate::mark::Mark;
use crate::annotate::rect::PixelRect;
use crate::annotate::testing::{as_image, block, line};
use crate::ui::color;

const LABEL: PixelRect = PixelRect { x: 12, y: 10, width: 30, height: 12 };
const MOVED: PixelRect = PixelRect { x: 30, y: 28, width: 30, height: 12 };

type Change = fn(&mut Sheet) -> Option<PixelRect>;

fn level_line() -> Mark {
    line(color::GREEN, 8.0, [4.0, 16.0], [60.0, 16.0]).into()
}

fn upright_line() -> Mark {
    line(color::YELLOW, 8.0, [27.0, 2.0], [27.0, 46.0]).into()
}

fn words_between_two_lines() -> Sheet {
    let mut sheet = sheet(64, 48);
    sheet.commit(level_line());
    sheet.commit(block(color::RED, LABEL));
    sheet.commit(upright_line());
    sheet
}

#[test]
fn words_and_lines_stack_in_order() {
    let mut sheet = sheet(64, 48);
    sheet.commit(level_line());
    assert_eq!(sheet.commit(block(color::RED, LABEL)), LABEL);
    sheet.commit(upright_line());
    let image = sheet.image();
    assert_eq!(image[(8, 16)], color::GREEN);
    assert_eq!(image[(20, 16)], color::RED, "the words cover the line made before them");
    assert_eq!(image[(27, 16)], color::YELLOW, "and the line made after them covers the words");
    assert_eq!(sheet.marks()[1].text().map(Lettering::area), Some(LABEL));
    assert!(shows(&sheet, sheet.marks()));
}

#[test]
fn undo_and_redo_a_block() {
    let mut sheet = sheet(64, 48);
    let words = Mark::from(block(color::RED, LABEL));
    sheet.commit(level_line());
    let inked = sheet.commit(words.clone());
    assert_eq!(sheet.undo(), Some(inked));
    assert!(shows(&sheet, &[level_line()]) && sheet.can_redo());
    assert_eq!(sheet.image()[(20, 16)], color::GREEN, "the line underneath shows through again");
    assert_eq!(sheet.redo(), Some(inked));
    assert!(shows(&sheet, &[level_line(), words]) && !sheet.can_redo());
    assert_eq!(sheet.image()[(20, 16)], color::RED);
}

#[test]
fn replaced_block_stays_under_lines() {
    let mut sheet = words_between_two_lines();
    let recoloured = Mark::from(block(color::PEARL, LABEL));
    assert_eq!(sheet.replace(1, recoloured.clone()), Some(LABEL));
    assert_eq!(sheet.marks(), [level_line(), recoloured, upright_line()]);
    assert_eq!((sheet.image()[(20, 16)], sheet.image()[(27, 16)]), (color::PEARL, color::YELLOW));
    assert!(shows(&sheet, sheet.marks()));
}

#[test]
fn moved_block_restores_old_place() {
    let mut sheet = words_between_two_lines();
    assert_eq!(sheet.replace(1, block(color::RED, MOVED)), Some(LABEL.union(MOVED)));
    assert_eq!((sheet.image()[(20, 16)], sheet.image()[(40, 34)]), (color::GREEN, color::RED));
    assert!(shows(&sheet, sheet.marks()));
}

#[test]
fn undoing_replace_restores_words() {
    let mut sheet = words_between_two_lines();
    let (old, new) = (sheet.marks()[1].clone(), Mark::from(block(color::INK, MOVED)));
    let redrawn = sheet.replace(1, new.clone());
    assert_eq!(sheet.undo(), redrawn);
    assert_eq!(sheet.marks(), [level_line(), old.clone(), upright_line()]);
    assert!(shows(&sheet, sheet.marks()) && sheet.can_redo());
    assert_eq!(sheet.redo(), redrawn);
    assert_eq!(sheet.marks(), [level_line(), new, upright_line()]);
    assert!(shows(&sheet, sheet.marks()) && !sheet.can_redo());
    sheet.undo();
    sheet.undo();
    assert_eq!(sheet.marks(), [level_line(), old]);
    while sheet.undo().is_some() {}
    assert_eq!(sheet.image(), as_image(sheet.base()));
    assert!(sheet.marks().is_empty() && !sheet.can_undo());
}

#[test]
fn replace_forgets_the_undone() {
    let mut sheet = words_between_two_lines();
    sheet.undo();
    let before = sheet.revision();
    assert!(sheet.can_redo());
    sheet.replace(1, block(color::INK, MOVED));
    assert_eq!(sheet.revision(), before + 1);
    assert!(!sheet.can_redo() && sheet.redo().is_none());
    assert_eq!(sheet.marks().len(), 2);
}

#[test]
fn emptied_block_keeps_its_place() {
    let mut sheet = words_between_two_lines();
    assert_eq!(sheet.replace(1, Lettering::empty()), Some(LABEL));
    assert_eq!(sheet.marks().len(), 3);
    assert!(shows(&sheet, &[level_line(), upright_line()]), "only the lines are left on the picture");
    assert_eq!(sheet.words_at(pos2(20.0, 16.0)), None);
    assert_eq!(sheet.undo(), Some(LABEL));
    assert_eq!(sheet.words_at(pos2(20.0, 16.0)), Some(1));
    assert!(shows(&sheet, sheet.marks()));
}

#[test]
fn lifted_block_hidden_until_settled() {
    let mut sheet = words_between_two_lines();
    assert_eq!(sheet.lift(1), Some(LABEL));
    assert!(shows(&sheet, &[level_line(), upright_line()]), "the picture is drawn without the lifted block");
    assert_eq!(sheet.marks().len(), 3, "which is still one of its marks");
    assert_eq!(sheet.settle(), LABEL);
    assert!(shows(&sheet, sheet.marks()));
    assert_eq!(sheet.settle(), PixelRect::EMPTY, "nothing is left to settle");
}

#[test]
fn lifting_another_settles_the_first() {
    let mut sheet = words_between_two_lines();
    sheet.commit(block(color::INK, MOVED));
    sheet.lift(1);
    assert_eq!(sheet.lift(3), Some(LABEL.union(MOVED)));
    assert!(shows(&sheet, &sheet.marks()[..3]));
    assert_eq!(sheet.settle(), MOVED);
    assert!(shows(&sheet, sheet.marks()));
}

#[test]
fn lifting_and_settling_are_not_changes() {
    let mut sheet = words_between_two_lines();
    sheet.undo();
    let unchanged = (sheet.revision(), sheet.can_undo(), sheet.can_redo(), sheet.marks().to_vec());
    sheet.lift(1);
    assert_eq!((sheet.revision(), sheet.can_undo(), sheet.can_redo(), sheet.marks().to_vec()), unchanged);
    sheet.settle();
    assert_eq!((sheet.revision(), sheet.can_undo(), sheet.can_redo(), sheet.marks().to_vec()), unchanged);
    assert!(sheet.redo().is_some(), "what was undone can still be put back");
}

#[test]
fn changes_settle_a_lifted_block() {
    let changes: [(&str, Change); 4] = [
        ("a commit", |sheet| Some(sheet.commit(block(color::INK, MOVED)))),
        ("a replacement", |sheet| sheet.replace(0, upright_line())),
        ("an undo", Sheet::undo),
        ("a redo", Sheet::redo),
    ];
    for (change, make) in changes {
        let mut sheet = words_between_two_lines();
        sheet.commit(block(color::PEARL, MOVED));
        sheet.undo();
        sheet.lift(1);
        let redrawn = make(&mut sheet).expect("a change");
        assert!(shows(&sheet, sheet.marks()), "{change}");
        assert_eq!(redrawn.union(LABEL), redrawn, "the place of the lifted block is redrawn by {change}");
        assert_eq!(sheet.settle(), PixelRect::EMPTY, "{change}");
    }
}

#[test]
fn replacing_lifted_shows_new_words() {
    let mut sheet = words_between_two_lines();
    sheet.lift(1);
    assert_eq!(sheet.replace(1, block(color::INK, MOVED)), Some(LABEL.union(MOVED)));
    assert!(shows(&sheet, sheet.marks()));
    assert_eq!(sheet.settle(), PixelRect::EMPTY);
    assert_eq!((sheet.image()[(20, 16)], sheet.image()[(40, 34)]), (color::GREEN, color::INK));
    sheet.lift(1);
    assert_eq!(sheet.undo(), Some(LABEL.union(MOVED)));
    assert!(shows(&sheet, sheet.marks()));
    assert_eq!(sheet.image()[(20, 16)], color::RED);
}

#[test]
fn missing_mark_cannot_be_touched() {
    let mut fresh = sheet(64, 48);
    assert_eq!((fresh.replace(0, block(color::RED, LABEL)), fresh.lift(0)), (None, None));
    assert_eq!((fresh.revision(), fresh.can_undo()), (0, false));
    let mut sheet = words_between_two_lines();
    let revision = sheet.revision();
    assert_eq!((sheet.replace(3, block(color::RED, MOVED)), sheet.lift(3)), (None, None));
    assert_eq!((sheet.revision(), sheet.marks().len()), (revision, 3));
    assert!(shows(&sheet, sheet.marks()));
}

#[test]
fn click_finds_topmost_block() {
    let mut sheet = words_between_two_lines();
    sheet.commit(block(color::PEARL, MOVED));
    sheet.commit(block(color::INK, rect(36, 16, 12, 16)));
    assert_eq!(sheet.words_at(pos2(14.0, 12.0)), Some(1));
    assert_eq!(
        sheet.words_at(pos2(27.0, 16.0)),
        Some(1),
        "a line drawn over the words does not keep a click from them"
    );
    assert_eq!((sheet.words_at(pos2(40.0, 20.0)), sheet.words_at(pos2(40.0, 30.0))), (Some(4), Some(4)));
    assert_eq!(sheet.words_at(pos2(55.0, 36.0)), Some(3));
    assert_eq!(sheet.words_at(pos2(27.0, 44.0)), None, "a line alone is not words");
    assert_eq!(sheet.words_at(pos2(5.0, 5.0)), None);
    sheet.undo();
    assert_eq!((sheet.words_at(pos2(40.0, 20.0)), sheet.words_at(pos2(40.0, 30.0))), (Some(1), Some(3)));
}
