use egui::{Event, Key, Modifiers};

use super::super::super::bench::{button, key_event};
use super::super::across;
use super::{GREETING, away_from_the_middle, placed, typing, with_the_text_tool, written};
use crate::annotate::{Brush, Outcome};

fn typed(letters: &str) -> Event {
    Event::Text(letters.to_owned())
}

#[test]
fn unread_typing_is_placed() {
    let mut bench = typing("H");
    let mut unread = vec![typed("i")];
    assert_eq!(bench.editor.finish_what_is_in_progress(&mut unread), [Outcome::Changed(1)]);
    assert_eq!(placed(&bench)[0].words().text, GREETING);
    assert_eq!(unread, [], "read once, it is not left for the next frame");
}

#[test]
fn unread_keys_type_not_shortcuts() {
    let mut bench = typing("H");
    let line_break = key_event(Key::Enter, Modifiers::SHIFT, true);
    let digit = [key_event(Key::Num2, Modifiers::NONE, true), typed("2")];
    let mut unread = vec![digit[0].clone(), digit[1].clone(), line_break, Event::Paste("!".to_owned())];
    assert_eq!(bench.editor.finish_what_is_in_progress(&mut unread), [Outcome::Changed(1)]);
    assert_eq!(placed(&bench)[0].words().text, "H2\n!");
    assert_eq!(unread, []);
    assert_eq!(bench.frame(unread), []);
    assert_eq!(bench.editor.brush, Brush::default(), "the digit was typed and picked no swatch");
}

#[test]
fn unread_finishing_key_never_closes() {
    for key in [Key::Enter, Key::Escape] {
        let mut bench = typing("H");
        let mut unread = vec![typed("i"), key_event(key, Modifiers::NONE, true), typed("x")];
        assert_eq!(bench.editor.finish_what_is_in_progress(&mut unread), [Outcome::Changed(1)], "{key:?}");
        assert_eq!(placed(&bench)[0].words().text, GREETING, "{key:?}");
        assert_eq!(unread, [typed("x")], "{key:?}: what came after the block ended is left for the frame");
        assert_eq!(bench.frame(unread), [], "{key:?} is not left to close the window");
        assert_eq!(placed(&bench).len(), 1, "{key:?}");
    }
}

#[test]
fn unread_undo_takes_block_back() {
    let mut bench = typing("H");
    let undo = key_event(Key::Z, Modifiers::COMMAND, true);
    let mut unread = vec![typed("i"), undo.clone(), typed("x")];
    assert_eq!(bench.editor.finish_what_is_in_progress(&mut unread), [Outcome::Changed(1)]);
    assert_eq!(placed(&bench)[0].words().text, GREETING);
    assert_eq!(unread, [undo, typed("x")]);
    assert_eq!(bench.frame(unread), [Outcome::Changed(2)]);
    assert!(bench.editor.job().marks.is_empty() && bench.editor.typing.is_none());
}

#[test]
fn stops_taking_at_a_press() {
    let mut bench = typing("H");
    let elsewhere = away_from_the_middle(&bench);
    let left = [Event::PointerMoved(elsewhere), button(elsewhere, true), typed("z")];
    let mut unread = vec![left[0].clone(), typed("i"), left[1].clone(), left[2].clone()];
    assert_eq!(bench.editor.finish_what_is_in_progress(&mut unread), [Outcome::Changed(1)]);
    assert_eq!(placed(&bench)[0].words().text, GREETING);
    assert_eq!(unread, left);
    assert_eq!(bench.frame(unread), [], "the press opens a new block and the late letter goes into it");
    assert_eq!(written(&bench), "z");
}

#[test]
fn no_block_takes_nothing_unread() {
    let waiting = [typed("x"), key_event(Key::Enter, Modifiers::NONE, true)];
    let mut idle = with_the_text_tool();
    let mut unread = waiting.to_vec();
    assert_eq!(idle.editor.finish_what_is_in_progress(&mut unread), []);
    assert_eq!(unread, waiting);
    let mut drawing = super::bench();
    let [from, middle, _] = across(&drawing);
    drawing.press(from);
    drawing.hover(middle);
    assert_eq!(drawing.editor.finish_what_is_in_progress(&mut unread), [Outcome::Changed(1)]);
    assert_eq!(unread, waiting, "a line being drawn reads no keys");
}
