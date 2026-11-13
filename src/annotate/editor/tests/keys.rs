use egui::{Key, Modifiers};

use super::super::bench::Bench;
use super::{CLOSERS, across, bench};
use crate::annotate::{Brush, Outcome};

fn in_the_middle_of_a_line() -> Bench {
    let mut bench = bench();
    let [from, middle, _] = across(&bench);
    bench.press(from);
    bench.hover(middle);
    assert!(bench.editor.pen.is_some());
    bench
}

#[test]
fn escape_held_after_drop_stays_open() {
    let mut bench = in_the_middle_of_a_line();
    assert_eq!(bench.hold(Key::Escape, Modifiers::NONE), []);
    assert!(bench.editor.pen.is_none());
    for repeat in 0..40 {
        assert_eq!(bench.hold(Key::Escape, Modifiers::NONE), [], "repeat {repeat} of the held key");
    }
    assert_eq!(bench.editor.revision(), 0);
    assert_eq!(bench.key(Key::Escape, Modifiers::NONE), [], "letting go is still part of the same press");
    assert_eq!(bench.key(Key::Escape, Modifiers::NONE), [Outcome::Close], "a new press with no line to drop");
}

#[test]
fn held_key_asks_to_close_once() {
    for (key, modifiers) in CLOSERS {
        let mut bench = bench();
        assert_eq!(bench.hold(key, modifiers), [Outcome::Close], "{key:?}");
        assert_eq!(bench.hold(key, modifiers), [], "{key:?} repeating");
        assert_eq!(bench.hold(key, modifiers), [], "{key:?} repeating");
    }
}

#[test]
fn closing_keys_finish_the_line() {
    for (key, modifiers) in [(Key::Enter, Modifiers::NONE), (Key::W, Modifiers::COMMAND)] {
        let mut bench = in_the_middle_of_a_line();
        assert_eq!(bench.hold(key, modifiers), [Outcome::Changed(1), Outcome::Close], "{key:?}");
        let job = bench.editor.job();
        assert_eq!((job.revision, job.marks.len()), (1, 1), "{key:?}");
        assert!(bench.editor.pen.is_none());
    }
}

#[test]
fn window_close_finishes_the_line() {
    let mut bench = in_the_middle_of_a_line();
    assert_eq!(bench.editor.close(), [Outcome::Changed(1), Outcome::Close]);
    assert_eq!(bench.editor.job().marks.len(), 1);
    assert_eq!(bench.editor.close(), [Outcome::Close], "with no line in progress it only closes");
}

#[test]
fn held_brackets_and_undo_repeat() {
    let mut bench = bench();
    (0..3).for_each(|_| drop(bench.hold(Key::CloseBracket, Modifiers::NONE)));
    assert_eq!(bench.editor.brush.size(), Brush::DEFAULT_SIZE + 3.0);
    let [from, middle, to] = across(&bench);
    bench.drag(&[from, middle]);
    bench.drag(&[middle, to]);
    assert_eq!(bench.hold(Key::Z, Modifiers::COMMAND), [Outcome::Changed(3)]);
    assert_eq!(bench.hold(Key::Z, Modifiers::COMMAND), [Outcome::Changed(4)]);
    assert_eq!(bench.editor.job().marks.len(), 0);
}

#[test]
fn tab_and_space_press_nothing() {
    let mut bench = bench();
    for tabs in 1..=12 {
        assert_eq!(bench.key(Key::Tab, Modifiers::NONE), []);
        assert_eq!(bench.key(Key::Space, Modifiers::NONE), [], "after {tabs} presses of tab");
    }
    assert_eq!(bench.editor.brush, Brush::default());
}
