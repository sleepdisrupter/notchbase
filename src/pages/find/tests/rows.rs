use egui::{Event, Id, Key};

use super::{Run, opened, row};
use crate::bridge::{Action, FindView, How};
use crate::pages::find::row::id_of;
use crate::pages::harness::{away, key, typed};

#[test]
fn click_opens_row_keeps_keyboard() {
    let mut run = Run::new();
    run.enter();
    run.answer("", &["a", "b"]);
    run.settle();
    let actions = run.click(id_of(row("b").key));
    assert_eq!(actions, [opened("b", "", 1, How::Open)]);
    assert!(run.page.focus.is_some());
}

#[test]
fn selected_buttons_reveal_and_shelve() {
    let mut run = Run::new();
    run.enter();
    run.answer("", &["a"]);
    run.settle();
    let id = id_of(row("a").key);
    assert_eq!(run.click(id.with("reveal")), [opened("a", "", 0, How::Reveal)]);
    assert_eq!(run.click(id.with("shelve")), [opened("a", "", 0, How::Shelve)]);
    assert!(run.click(id.with("shelve")).is_empty(), "the checkmark is not a button");
}

#[test]
fn access_row_can_be_dismissed() {
    let mut run = Run::new();
    run.view.nudge = true;
    run.enter();
    let access = Id::new("notchbase.find.access");
    assert_eq!(run.click(access.with("hide")), [Action::FindHideAccess]);
    assert_eq!(run.click(access.with("allow")), [Action::AllowFullDiskAccess]);
    run.frame(vec![typed("c")]);
    assert!(run.settle().is_empty());
    assert!(!run.harness.drawn(access.with("hide")), "a query hides it");
}

#[test]
fn access_row_folds_while_typing() {
    let mut run = Run::new();
    run.view.nudge = true;
    run.enter();
    let access = Id::new("notchbase.find.access");
    let full = run.page.height_for(&run.view);
    run.frame(vec![typed("c")]);
    run.frame(Vec::new());
    let folding = run.page.height_for(&run.view);
    assert!(folding < full && folding > super::super::FIELD_H, "{folding} of {full}");
    assert!(run.harness.drawn(access.with("hide")), "still drawn while it folds");
    run.settle();
    assert!(!run.harness.drawn(access.with("hide")));
    assert!(!run.page.is_animating());
}

#[test]
fn rows_keep_ids_across_answers() {
    let mut run = Run::new();
    run.enter();
    run.answer("", &["a", "b"]);
    run.settle();
    let before = run.harness.rect_of(id_of(row("b").key));
    run.answer("", &["a", "b", "c"]);
    run.settle();
    assert_eq!(run.harness.rect_of(id_of(row("b").key)), before);
}

#[test]
fn still_pointer_keeps_selection() {
    let mut run = Run::new();
    run.enter();
    run.answer("", &["a", "b", "c"]);
    run.settle();
    let over_c = run.harness.rect_of(id_of(row("c").key)).center();
    run.frame(vec![Event::PointerMoved(away())]);
    run.frame(vec![Event::PointerMoved(over_c)]);
    assert_eq!(run.page.select.index, 2, "a moving pointer picks the row");
    run.frame(vec![key(Key::ArrowUp)]);
    run.settle();
    assert_eq!(run.page.select.index, 1, "a still one leaves the keyboard's choice alone");
    run.frame(vec![Event::PointerMoved(away())]);
    assert_eq!(run.page.select.index, 1);
}

#[test]
fn pointed_row_scrolls_into_view() {
    let names: Vec<String> = (0..20).map(|index| format!("r{index}")).collect();
    let first: Vec<&str> = names.iter().map(String::as_str).collect();
    let mut run = Run::new();
    run.enter();
    run.frame(vec![typed("r")]);
    run.answer("r", &first);
    run.settle();
    run.frame(vec![typed("1")]);
    let over_r3 = run.harness.rect_of(id_of(row("r3").key)).center();
    run.frame(vec![Event::PointerMoved(away())]);
    run.frame(vec![Event::PointerMoved(over_r3)]);
    assert_eq!(run.page.select.index, 3);
    let mut second: Vec<&str> = first.iter().copied().filter(|name| *name != "r3").collect();
    second.insert(15, "r3");
    run.answer("r1", &second);
    run.settle();
    assert!(run.harness.drawn(id_of(row("r3").key)), "the row Enter would open is on screen");
    assert_eq!(run.frame(vec![key(Key::Enter)]), [opened("r3", "r1", 15, How::Open)]);
}

#[test]
fn turning_find_on() {
    let mut run = Run::new();
    run.view = FindView::default();
    run.enter();
    assert_eq!(run.click(Id::new("notchbase.find.on")), [Action::SetFindEnabled(true)]);
}
