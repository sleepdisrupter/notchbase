mod words;

use super::*;
use crate::annotate::composite::composite;
use crate::annotate::testing::{as_image, line, solid, textured, through};
use crate::ui::color;

fn sheet(width: usize, height: usize) -> Sheet {
    Sheet::new(Arc::new(textured(width, height))).expect("a sheet")
}

fn shows(sheet: &Sheet, marks: &[Mark]) -> bool {
    sheet.image() == as_image(&composite(sheet.base(), marks))
}

fn rect(x: usize, y: usize, width: usize, height: usize) -> PixelRect {
    PixelRect { x, y, width, height }
}

#[test]
fn fresh_sheet_shows_the_screenshot() {
    let fresh = sheet(48, 32);
    assert_eq!(fresh.image(), as_image(fresh.base()));
    assert_eq!((fresh.size(), fresh.revision(), fresh.marks().len()), ([48, 32], 0, 0));
    assert!(!fresh.can_undo() && !fresh.can_redo());
}

#[test]
fn pixelless_screenshot_refused() {
    let empty = Screenshot { width: 0, height: 0, rgba: Vec::new() };
    let torn = Screenshot { width: 4, height: 4, rgba: vec![0; 60] };
    assert!(Sheet::new(Arc::new(empty)).is_none() && Sheet::new(Arc::new(torn)).is_none());
}

#[test]
fn commit_inks_and_reports_pixels() {
    let mut sheet = sheet(64, 48);
    let stroke = line(color::RED, 6.0, [20.0, 20.0], [40.0, 20.0]);
    let dirty = sheet.commit(stroke.clone());
    assert_eq!(dirty, rect(16, 16, 28, 8));
    assert_eq!(sheet.marks(), [Mark::from(stroke)]);
    assert!(shows(&sheet, sheet.marks()));
    let (after, before) = (sheet.image(), as_image(sheet.base()));
    assert_eq!(after[(30, 20)], color::RED);
    for (index, (inked, plain)) in after.pixels.iter().zip(&before.pixels).enumerate() {
        let (x, y) = (index % 64, index / 64);
        let inside = (dirty.x..dirty.right()).contains(&x) && (dirty.y..dirty.bottom()).contains(&y);
        assert!(inside || inked == plain, "{x},{y}");
    }
}

#[test]
fn undo_and_redo_the_last_line() {
    let mut sheet = sheet(64, 48);
    let under = Mark::from(line(color::GREEN, 9.0, [8.0, 8.0], [56.0, 40.0]));
    let over = Mark::from(line(color::RED, 5.0, [8.0, 40.0], [56.0, 8.0]));
    sheet.commit(under.clone());
    let inked = sheet.commit(over.clone());
    assert!(sheet.can_undo() && !sheet.can_redo());
    assert_eq!(sheet.undo(), Some(inked));
    assert!(shows(&sheet, std::slice::from_ref(&under)) && sheet.can_redo());
    assert_eq!(sheet.image()[(32, 24)], color::GREEN, "the line underneath shows through again");
    assert_eq!(sheet.redo(), Some(inked));
    assert!(shows(&sheet, &[under, over]) && !sheet.can_redo());
    assert_eq!(sheet.image()[(32, 24)], color::RED);
}

#[test]
fn undoing_all_restores_screenshot() {
    let mut sheet = sheet(64, 48);
    sheet.commit(line(color::GREEN, 9.0, [8.0, 8.0], [56.0, 40.0]));
    sheet.commit(through(color::INK, 14.0, &[[30.0, 30.0]]));
    sheet.commit(line(color::PEARL, 3.0, [-20.0, 24.0], [90.0, 24.0]));
    while sheet.undo().is_some() {}
    assert_eq!(sheet.image(), as_image(sheet.base()));
    assert!(!sheet.can_undo() && sheet.undo().is_none());
    while sheet.redo().is_some() {}
    assert_eq!(sheet.marks().len(), 3);
    assert!(shows(&sheet, sheet.marks()) && sheet.redo().is_none());
}

#[test]
fn new_line_forgets_the_undone() {
    let mut sheet = sheet(64, 48);
    sheet.commit(line(color::GREEN, 9.0, [8.0, 8.0], [56.0, 40.0]));
    sheet.commit(line(color::RED, 5.0, [8.0, 40.0], [56.0, 8.0]));
    sheet.undo();
    let replacement = Mark::from(line(color::YELLOW, 5.0, [32.0, 4.0], [32.0, 44.0]));
    sheet.commit(replacement.clone());
    assert!(!sheet.can_redo() && sheet.redo().is_none());
    assert_eq!(sheet.marks().last(), Some(&replacement));
    assert_eq!(sheet.marks().len(), 2);
    assert!(shows(&sheet, sheet.marks()));
}

#[test]
fn revision_moves_with_each_change() {
    let mut sheet = sheet(64, 48);
    let mut seen = vec![sheet.revision()];
    sheet.commit(line(color::GREEN, 9.0, [8.0, 8.0], [56.0, 40.0]));
    seen.push(sheet.revision());
    sheet.undo();
    seen.push(sheet.revision());
    sheet.redo();
    seen.push(sheet.revision());
    sheet.commit(line(color::RED, 4.0, [-90.0, -90.0], [-50.0, -60.0]));
    seen.push(sheet.revision());
    assert!(seen.windows(2).all(|pair| pair[0] < pair[1]), "{seen:?}");
    assert!(sheet.redo().is_none());
    assert_eq!(Some(&sheet.revision()), seen.last());
}

#[test]
fn off_image_line_changes_no_pixel() {
    let mut sheet = sheet(64, 48);
    assert!(sheet.commit(line(color::RED, 4.0, [-90.0, -90.0], [-50.0, -60.0])).is_empty());
    assert_eq!(sheet.image(), as_image(sheet.base()));
    assert!(sheet.can_undo());
    assert_eq!(sheet.undo(), Some(PixelRect::EMPTY));
}

#[test]
fn patch_matches_the_whole_image() {
    let mut sheet = sheet(64, 48);
    let dirty = sheet.commit(line(color::RED, 6.0, [20.0, 20.0], [40.0, 20.0]));
    let (whole, patch) = (sheet.image(), sheet.patch(dirty).expect("a patch"));
    assert_eq!(patch.size, dirty.size());
    assert_eq!(patch, whole.region_by_pixels(dirty.origin(), dirty.size()));
    assert_eq!(sheet.patch(PixelRect::of_image([64, 48])), Some(whole.clone()));
    let hanging = sheet.patch(rect(60, 40, 30, 30)).expect("a patch");
    assert_eq!(hanging, whole.region_by_pixels([60, 40], [4, 8]));
    assert_eq!(sheet.patch(PixelRect::EMPTY), None);
    assert_eq!(sheet.patch(rect(64, 0, 5, 5)), None);
}

#[test]
fn plain_base_takes_exact_ink() {
    let mut sheet = Sheet::new(Arc::new(solid(32, 32, [255, 255, 255]))).expect("a sheet");
    sheet.commit(through(color::YELLOW, 10.0, &[[16.0, 16.0]]));
    assert_eq!(sheet.image()[(16, 16)], color::YELLOW);
    assert_eq!(sheet.image()[(2, 2)], Color32::WHITE);
}
