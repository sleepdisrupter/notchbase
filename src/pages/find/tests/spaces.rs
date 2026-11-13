use egui::{Id, Key};

use super::{Run, opened};
use crate::bridge::{Action, How};
use crate::pages::harness::{key, typed};

const ACCESS: &str = "notchbase.find.access";

fn field_holds(run: &Run) -> &str {
    run.page.focus.as_ref().map_or("", |focus| focus.text())
}

fn chosen(run: &Run) -> &str {
    &run.page.shown.rows[run.page.select.index].name
}

#[test]
fn lone_space_is_no_question() {
    let mut run = Run::new();
    run.view.nudge = true;
    run.enter();
    run.answer("", &["a", "b"]);
    run.settle();
    assert_eq!(run.frame(vec![typed(" ")]), [], "nothing is asked");
    assert_eq!((run.page.draft.as_str(), field_holds(&run)), ("", ""), "and nothing is kept");
    assert_eq!(run.frame(vec![typed("  "), typed(" ")]), []);
    assert!(run.settle().is_empty());
    assert!(run.harness.drawn(Id::new(ACCESS).with("hide")), "the access row of the empty field stays");
    assert_eq!(run.frame(vec![key(Key::Enter)]), [opened("a", "", 0, How::Open)], "the list is still the answer");
    assert_eq!(run.frame(vec![key(Key::Escape)]), [Action::FindClose], "and escape has nothing to clear");
}

#[test]
fn leading_space_dropped_inner_kept() {
    let mut run = Run::new();
    run.enter();
    assert_eq!(run.frame(vec![typed(" ")]), []);
    assert_eq!(run.frame(vec![typed("c")]), [Action::FindQuery("c".into())]);
    assert_eq!(run.frame(vec![typed(" ")]), [Action::FindQuery("c ".into())], "every edit of a text still asks");
    assert_eq!(run.frame(vec![typed("d")]), [Action::FindQuery("c d".into())]);
    assert_eq!((run.page.draft.as_str(), field_holds(&run)), ("c d", "c d"));
}

#[test]
fn erasing_to_spaces_clears_once() {
    let mut run = Run::new();
    run.enter();
    run.frame(vec![typed("a")]);
    assert_eq!(run.frame(vec![key(Key::ArrowLeft), typed(" ")]), [Action::FindQuery(" a".into())]);
    let erased = run.frame(vec![key(Key::ArrowRight), key(Key::Backspace)]);
    assert_eq!(erased, [Action::FindQuery(String::new())], "what is left is no text");
    assert_eq!((run.page.draft.as_str(), field_holds(&run)), ("", ""));
    assert!(run.settle().is_empty(), "it is said once");
    assert_eq!(run.frame(vec![key(Key::Escape)]), [Action::FindClose]);
}

#[test]
fn trailing_space_keeps_choice() {
    let mut run = Run::new();
    run.enter();
    run.frame(vec![typed("ca")]);
    run.answer("ca", &["one", "two", "three"]);
    run.settle();
    run.frame(vec![key(Key::ArrowDown), key(Key::ArrowDown)]);
    assert_eq!(run.frame(vec![typed(" ")]), [Action::FindQuery("ca ".into())], "it is still asked");
    assert_eq!(chosen(&run), "three");
    run.answer("ca ", &["one", "three", "two"]);
    run.settle();
    assert_eq!(chosen(&run), "three", "the choice follows its row into the new answer");
    assert_eq!(run.frame(vec![key(Key::Enter)]), [opened("three", "ca ", 1, How::Open)]);
    run.frame(vec![key(Key::Backspace)]);
    assert_eq!(chosen(&run), "three", "taking the space away changes nothing either");
    run.frame(vec![typed("r")]);
    assert_eq!(run.page.select.index, 0, "a letter gives the choice back to the top row");
}

#[test]
fn enter_after_space_opens_shown_row() {
    let mut run = Run::new();
    run.enter();
    run.frame(vec![typed("ca")]);
    run.answer("ca", &["one", "two", "three"]);
    run.settle();
    run.frame(vec![key(Key::ArrowDown)]);
    let asked = run.frame(vec![typed(" "), key(Key::Enter)]);
    assert_eq!(asked, [Action::FindQuery("ca ".into())], "the rows on screen answer the text without the space");
    run.answer("ca ", &["zero", "one", "two"]);
    assert_eq!(run.frame(Vec::new()), [opened("two", "ca ", 2, How::Open)], "the chosen row, where it is now");
    assert!(run.settle().is_empty(), "it fires once");
}
