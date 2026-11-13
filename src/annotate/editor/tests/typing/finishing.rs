use egui::{Event, Key, Modifiers};

use super::super::super::bench::{button, key_event};
use super::super::across;
use super::{Bench, GREETING, away_from_the_middle, placed, typing, with_the_text_tool};
use crate::annotate::Outcome;

const REDO: Modifiers = Modifiers::COMMAND.plus(Modifiers::SHIFT);

fn typing_after_a_line_was_drawn_and_undone() -> Bench {
    let mut bench = super::bench();
    bench.drag(&across(&bench));
    assert_eq!(bench.key(Key::Z, Modifiers::COMMAND), [Outcome::Changed(2)]);
    bench.key(Key::T, Modifiers::NONE);
    bench.click(bench.fit().rect().center());
    bench
}

#[test]
fn undo_takes_whole_block_back() {
    let mut bench = typing(GREETING);
    assert_eq!(bench.key(Key::Z, Modifiers::COMMAND), [Outcome::Changed(2)]);
    assert!(bench.editor.typing.is_none() && bench.editor.job().marks.is_empty());
    assert_eq!(bench.key(Key::Z, Modifiers::COMMAND), [], "the block went in one step");
    assert_eq!(bench.key(Key::Z, REDO), [Outcome::Changed(3)]);
    assert_eq!(placed(&bench)[0].words().text, GREETING);
}

#[test]
fn undo_on_empty_block_steps_back() {
    let mut bench = super::bench();
    bench.drag(&across(&bench));
    bench.key(Key::T, Modifiers::NONE);
    bench.click(bench.fit().rect().center());
    assert_eq!(bench.key(Key::Z, Modifiers::COMMAND), [Outcome::Changed(2)]);
    assert!(bench.editor.typing.is_none() && bench.editor.job().marks.is_empty());
}

#[test]
fn redo_while_typing_finds_nothing() {
    let mut bench = typing_after_a_line_was_drawn_and_undone();
    bench.type_text(GREETING);
    assert_eq!(bench.key(Key::Z, REDO), [Outcome::Changed(3)]);
    assert!(bench.editor.typing.is_none());
    assert_eq!((bench.editor.job().marks.len(), placed(&bench).len()), (1, 1), "the words took the line's place");
    let mut empty = typing_after_a_line_was_drawn_and_undone();
    assert_eq!(empty.key(Key::Z, REDO), [Outcome::Changed(3)]);
    assert!(empty.editor.typing.is_none());
    assert_eq!((empty.editor.job().marks.len(), placed(&empty).len()), (1, 0), "an empty block is in nobody's way");
}

#[test]
fn closing_finishes_the_block() {
    let mut bench = typing(GREETING);
    assert_eq!(bench.hold(Key::W, Modifiers::COMMAND), [Outcome::Changed(1), Outcome::Close]);
    assert!(bench.editor.typing.is_none());
    assert_eq!(placed(&bench)[0].words().text, GREETING);
    let mut bench = typing(GREETING);
    let done = bench.layout().done.center();
    assert_eq!(bench.click(done), [Outcome::Changed(1), Outcome::Close]);
    assert_eq!((bench.editor.job().revision, placed(&bench).len()), (1, 1));
}

#[test]
fn window_close_finishes_the_block() {
    let mut bench = typing(GREETING);
    assert_eq!(bench.editor.close(), [Outcome::Changed(1), Outcome::Close]);
    assert_eq!(placed(&bench).len(), 1);
    assert_eq!(bench.editor.close(), [Outcome::Close], "with no block open it only closes");
    let mut empty = typing("");
    assert_eq!(empty.editor.close(), [Outcome::Close], "an empty block leaves nothing to report");
    assert!(empty.editor.typing.is_none());
}

#[test]
fn losing_focus_finishes_the_block() {
    let mut bench = typing(GREETING);
    assert_eq!(bench.frame(vec![Event::WindowFocused(false)]), [Outcome::Changed(1)]);
    assert!(bench.editor.typing.is_none());
    assert_eq!(placed(&bench)[0].words().text, GREETING);
    assert_eq!(bench.frame(vec![Event::WindowFocused(true)]), []);
    assert_eq!(bench.type_text("more"), [], "the block is closed, so nothing is typed into it");
    assert_eq!(bench.editor.revision(), 1);
}

#[test]
fn finishes_without_frame_or_closing() {
    let mut bench = typing(GREETING);
    assert_eq!(bench.editor.finish_what_is_in_progress(&mut Vec::new()), [Outcome::Changed(1)]);
    assert!(bench.editor.typing.is_none());
    assert_eq!(placed(&bench)[0].words().text, GREETING);
    assert_eq!(bench.editor.finish_what_is_in_progress(&mut Vec::new()), [], "nothing is left in progress");
    assert_eq!(bench.settle(), []);
}

#[test]
fn finishing_ends_a_line_too() {
    let mut bench = super::bench();
    let [from, middle, _] = across(&bench);
    bench.press(from);
    bench.hover(middle);
    assert_eq!(bench.editor.finish_what_is_in_progress(&mut Vec::new()), [Outcome::Changed(1)]);
    assert!(bench.editor.pen.is_none());
    assert_eq!(bench.editor.job().marks.len(), 1);
    assert_eq!(bench.release(middle), [], "the line was already ended");
}

#[test]
fn same_frame_words_are_placed() {
    let mut bench = typing(GREETING);
    let elsewhere = away_from_the_middle(&bench);
    let events = vec![Event::Text("!".to_owned()), button(elsewhere, true)];
    assert_eq!(bench.frame(events), [Outcome::Changed(1)]);
    assert_eq!(placed(&bench)[0].words().text, "Hi!");
    let mut bench = typing(GREETING);
    let events = vec![Event::Text("?".to_owned()), Event::WindowFocused(false)];
    assert_eq!(bench.frame(events), [Outcome::Changed(1)]);
    assert_eq!(placed(&bench)[0].words().text, "Hi?");
}

#[test]
fn same_frame_letter_is_kept() {
    let finishers = [(Key::Enter, Modifiers::NONE), (Key::Escape, Modifiers::NONE), (Key::W, Modifiers::COMMAND)];
    for (key, modifiers) in finishers {
        let mut bench = typing("H");
        let outcomes = bench.frame(vec![Event::Text("i".to_owned()), key_event(key, modifiers, true)]);
        assert_eq!(outcomes.first(), Some(&Outcome::Changed(1)), "{key:?}");
        assert_eq!(placed(&bench)[0].words().text, GREETING, "{key:?}");
    }
    let mut bench = typing("H");
    let undone = bench.frame(vec![Event::Text("i".to_owned()), key_event(Key::Z, Modifiers::COMMAND, true)]);
    assert_eq!(undone, [Outcome::Changed(2)]);
    bench.key(Key::Z, REDO);
    assert_eq!(placed(&bench)[0].words().text, GREETING);
}

#[test]
fn late_events_come_after_finish() {
    let mut bench = typing(GREETING);
    let typed_late = vec![key_event(Key::Enter, Modifiers::NONE, true), Event::Text("x".to_owned())];
    assert_eq!(bench.frame(typed_late), [Outcome::Changed(1)]);
    assert_eq!(placed(&bench)[0].words().text, GREETING);
    assert!(bench.editor.typing.is_none(), "the late letter found no block to go into");
    let mut bench = typing(GREETING);
    let elsewhere = away_from_the_middle(&bench);
    let pressed_late = vec![key_event(Key::Escape, Modifiers::NONE, true), button(elsewhere, true)];
    assert_eq!(bench.frame(pressed_late), [Outcome::Changed(1)]);
    assert_eq!(placed(&bench).len(), 1);
    assert!(bench.editor.typing.is_some(), "the late press opened a new block");
}

#[test]
fn rests_with_a_block_open() {
    let mut bench = with_the_text_tool();
    assert!(bench.editor.is_animating(), "the thumb glides to the text tool");
    bench.settle();
    assert!(!bench.editor.is_animating());
    bench.click(bench.fit().rect().center());
    bench.type_text(GREETING);
    assert_eq!(bench.settle(), []);
    assert!(!bench.editor.is_animating(), "a steady caret asks for no frames");
    bench.key(Key::Enter, Modifiers::NONE);
    assert_eq!(bench.settle(), []);
    assert!(!bench.editor.is_animating());
}
