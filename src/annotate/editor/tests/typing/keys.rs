use egui::{Event, Key, Modifiers};

use super::super::super::tool::Tool;
use super::super::CLOSERS;
use super::{GREETING, caret, open, placed, typing, with_the_text_tool, written};
use crate::annotate::{Brush, Outcome};
use crate::ui::color;

const HELD_REPEATS: usize = 40;
const TWO_LINES: &str = "ab\ncd";

fn typing_two_lines() -> super::Bench {
    let mut bench = typing("ab");
    assert_eq!(bench.key(Key::Enter, Modifiers::SHIFT), []);
    bench.type_text("cd");
    assert_eq!((written(&bench), caret(&bench)), (TWO_LINES, 5));
    bench
}

#[test]
fn shortcut_keys_type_while_typing() {
    let mut bench = typing("");
    let keys = [(Key::Num2, "2"), (Key::OpenBracket, "["), (Key::CloseBracket, "]"), (Key::B, "b"), (Key::T, "t")];
    for (key, letter) in keys {
        assert_eq!(bench.strike(key, letter), [], "{key:?}");
    }
    assert_eq!(bench.strike(Key::Space, " "), []);
    assert_eq!(bench.strike(Key::Num5, "5"), []);
    assert_eq!(written(&bench), "2[]bt 5");
    assert_eq!((bench.editor.brush, bench.editor.tool), (Brush::default(), Tool::Text));
}

#[test]
fn shortcuts_return_after_the_block() {
    let mut bench = typing(GREETING);
    bench.key(Key::Enter, Modifiers::NONE);
    let honey = Brush::default().with_color(color::YELLOW);
    assert_eq!(bench.strike(Key::Num2, "2"), [Outcome::Remember(honey)]);
    assert_eq!(bench.strike(Key::CloseBracket, "]"), [Outcome::Remember(honey.with_size(Brush::DEFAULT_SIZE + 1.0))]);
    assert_eq!(bench.strike(Key::B, "b"), []);
    assert_eq!(bench.editor.tool, Tool::Brush);
    assert!(bench.editor.typing.is_none() && placed(&bench).len() == 1, "and none of them typed anything");
}

#[test]
fn russian_bracket_letter_types() {
    let mut bench = typing("");
    assert_eq!(bench.strike(Key::OpenBracket, "х"), []);
    assert_eq!(bench.strike(Key::CloseBracket, "ъ"), []);
    assert_eq!((written(&bench), caret(&bench)), ("хъ", 2));
    assert_eq!(bench.editor.brush, Brush::default());
}

#[test]
fn shift_enter_breaks_enter_finishes() {
    let mut bench = typing("one");
    let one_line = open(&bench).frame().height();
    assert_eq!(bench.key(Key::Enter, Modifiers::SHIFT), []);
    assert_eq!(bench.type_text("two"), []);
    assert_eq!(written(&bench), "one\ntwo");
    assert_eq!(open(&bench).frame().height(), one_line * 2.0);
    assert_eq!(bench.key(Key::Enter, Modifiers::NONE), [Outcome::Changed(1)]);
    assert_eq!(placed(&bench)[0].words().text, "one\ntwo");
    assert!(bench.editor.typing.is_none());
}

#[test]
fn second_enter_closes() {
    let mut bench = typing(GREETING);
    assert_eq!(bench.key(Key::Enter, Modifiers::NONE), [Outcome::Changed(1)]);
    assert_eq!(bench.key(Key::Enter, Modifiers::NONE), [Outcome::Close]);
    let mut held = typing(GREETING);
    assert_eq!(held.hold(Key::Enter, Modifiers::NONE), [Outcome::Changed(1)]);
    for repeat in 0..HELD_REPEATS {
        assert_eq!(held.hold(Key::Enter, Modifiers::NONE), [], "repeat {repeat} of the held key");
    }
}

#[test]
fn held_shift_enter_breaks_once() {
    let mut bench = typing("a");
    (0..4).for_each(|_| drop(bench.hold(Key::Enter, Modifiers::SHIFT)));
    (0..4).for_each(|_| drop(bench.repeat_from_the_window(Key::Enter, Modifiers::SHIFT)));
    assert_eq!(written(&bench), "a\n");
    bench.let_go(Key::Enter, Modifiers::SHIFT);
    bench.key(Key::Enter, Modifiers::SHIFT);
    assert_eq!(written(&bench), "a\n\n");
}

#[test]
fn escape_keeps_words_never_closes() {
    let mut bench = typing(GREETING);
    assert_eq!(bench.hold(Key::Escape, Modifiers::NONE), [Outcome::Changed(1)]);
    assert!(bench.editor.typing.is_none());
    assert_eq!(placed(&bench)[0].words().text, GREETING);
    for repeat in 0..HELD_REPEATS {
        assert_eq!(bench.hold(Key::Escape, Modifiers::NONE), [], "repeat {repeat} of the held key");
    }
    assert_eq!(bench.key(Key::Escape, Modifiers::NONE), [], "letting go is still part of the same press");
    assert_eq!(bench.key(Key::Escape, Modifiers::NONE), [Outcome::Close], "a new press with no block to finish");
}

#[test]
fn text_tool_refocus_never_closes() {
    for (key, modifiers) in CLOSERS {
        let mut bench = with_the_text_tool();
        bench.frame(vec![Event::WindowFocused(false)]);
        bench.frame(vec![Event::WindowFocused(true)]);
        for repeat in 0..HELD_REPEATS {
            assert_eq!(bench.repeat_from_the_window(key, modifiers), [], "{key:?}, repeat {repeat}");
        }
        assert_eq!(bench.let_go(key, modifiers), [], "{key:?} let go");
        assert_eq!(bench.hold(key, modifiers), [Outcome::Close], "{key:?} pressed afresh");
    }
}

#[test]
fn erasing_empty_block_drops_it() {
    let mut bench = typing("abc");
    assert_eq!(bench.key(Key::Backspace, Modifiers::NONE), []);
    assert_eq!((written(&bench), caret(&bench)), ("ab", 2));
    bench.key(Key::ArrowLeft, Modifiers::NONE);
    assert_eq!(bench.key(Key::Delete, Modifiers::NONE), []);
    assert_eq!((written(&bench), caret(&bench)), ("a", 1));
    assert_eq!(bench.key(Key::Delete, Modifiers::NONE), [], "nothing lies after the caret");
    bench.key(Key::Backspace, Modifiers::NONE);
    assert_eq!((written(&bench), caret(&bench)), ("", 0));
    assert_eq!(bench.key(Key::Backspace, Modifiers::NONE), []);
    assert!(bench.editor.typing.is_none(), "erasing in an empty block drops it");
    assert_eq!((bench.editor.revision(), bench.editor.job().marks.len()), (0, 0));
    assert_eq!(bench.key(Key::Backspace, Modifiers::NONE), [], "with no block open it does nothing");
}

#[test]
fn held_letters_and_backspace_repeat() {
    let mut bench = typing("");
    (0..3).for_each(|_| drop(bench.frame(vec![Event::Text("a".to_owned())])));
    assert_eq!(written(&bench), "aaa");
    (0..2).for_each(|_| drop(bench.hold(Key::Backspace, Modifiers::NONE)));
    (0..1).for_each(|_| drop(bench.repeat_from_the_window(Key::Backspace, Modifiers::NONE)));
    assert_eq!(written(&bench), "");
}

#[test]
fn held_erase_stops_at_empty_block() {
    for key in [Key::Backspace, Key::Delete] {
        let mut bench = typing("ab");
        if key == Key::Delete {
            bench.key(Key::Home, Modifiers::NONE);
        }
        for repeat in 0..HELD_REPEATS {
            assert_eq!(bench.hold(key, Modifiers::NONE), [], "{key:?}, repeat {repeat}");
            assert_eq!(bench.repeat_from_the_window(key, Modifiers::NONE), [], "{key:?}, repeat {repeat}");
        }
        assert_eq!((written(&bench), bench.editor.revision()), ("", 0), "{key:?} still held");
        assert_eq!(bench.strike(Key::B, "b"), [], "{key:?}: what is typed next goes into the block");
        assert_eq!((written(&bench), bench.editor.tool), ("b", Tool::Text), "{key:?}");
        bench.let_go(key, Modifiers::NONE);
        bench.key(Key::Backspace, Modifiers::NONE);
        assert_eq!(written(&bench), "", "{key:?}");
        assert_eq!(bench.key(key, Modifiers::NONE), [], "{key:?} pressed afresh");
        assert!(bench.editor.typing.is_none(), "{key:?}: a new press in the empty block drops it");
        assert_eq!((bench.editor.revision(), bench.editor.job().marks.len()), (0, 0), "{key:?}");
    }
}

#[test]
fn arrows_home_end_move_caret() {
    let mut bench = typing_two_lines();
    let mut after = |key: Key| {
        assert_eq!(bench.key(key, Modifiers::NONE), [], "{key:?}");
        caret(&bench)
    };
    assert_eq!([Key::ArrowUp, Key::Home, Key::ArrowRight, Key::ArrowDown, Key::End].map(&mut after), [2, 0, 1, 4, 5]);
    assert_eq!([Key::ArrowLeft, Key::ArrowUp, Key::ArrowUp, Key::End].map(&mut after), [4, 1, 0, 2]);
    assert_eq!([Key::ArrowRight, Key::ArrowRight, Key::ArrowDown, Key::ArrowRight].map(&mut after), [3, 4, 5, 5]);
    bench.key(Key::Home, Modifiers::NONE);
    bench.type_text("X");
    assert_eq!((written(&bench), caret(&bench)), ("ab\nXcd", 4));
}

#[test]
fn command_arrows_jump_to_line_ends() {
    let mut bench = typing_two_lines();
    bench.key(Key::ArrowLeft, Modifiers::MAC_CMD);
    assert_eq!(caret(&bench), 3);
    bench.key(Key::ArrowUp, Modifiers::NONE);
    bench.key(Key::ArrowRight, Modifiers::MAC_CMD);
    assert_eq!(caret(&bench), 2);
}

#[test]
fn paste_keeps_only_line_breaks() {
    let mut bench = typing("");
    assert_eq!(bench.paste("one\r\ntwo\tthree\u{7}"), []);
    assert_eq!((written(&bench), caret(&bench)), ("one\ntwothree", 12));
    bench.key(Key::Home, Modifiers::NONE);
    bench.paste("2: ");
    assert_eq!(written(&bench), "one\n2: twothree");
}

#[test]
fn copy_and_cut_change_nothing() {
    let mut bench = typing(GREETING);
    for event in [Event::Copy, Event::Cut] {
        assert_eq!(bench.frame(vec![event]), []);
        assert!(bench.camera.asked_nothing_of_the_clipboard());
        assert_eq!((written(&bench), caret(&bench)), (GREETING, 2));
    }
    bench.key(Key::A, Modifiers::COMMAND);
    bench.key(Key::Tab, Modifiers::NONE);
    assert_eq!((written(&bench), caret(&bench)), (GREETING, 2));
}

#[test]
fn no_block_keys_stay_shortcuts() {
    let mut bench = with_the_text_tool();
    assert_eq!(bench.type_text("x"), []);
    assert_eq!(bench.paste(GREETING), []);
    assert_eq!(bench.key(Key::Backspace, Modifiers::NONE), []);
    assert!(bench.editor.typing.is_none() && bench.editor.job().marks.is_empty());
    let jade = Brush::default().with_color(color::GREEN);
    assert_eq!(bench.strike(Key::Num3, "3"), [Outcome::Remember(jade)]);
    assert!(bench.editor.typing.is_none());
}
