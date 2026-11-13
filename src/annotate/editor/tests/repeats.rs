use egui::{Event, Key, Modifiers};

use super::super::bench::{Bench, key_event};
use super::super::keys::forget_repeated_strikes;
use super::{CLOSERS, across, bench};
use crate::annotate::{Brush, Outcome};

const HELD_REPEATS: usize = 40;
const NUDGES: usize = 3;

fn with_the_keyboard_back() -> Bench {
    let mut bench = bench();
    bench.frame(vec![Event::WindowFocused(false)]);
    bench.frame(vec![Event::WindowFocused(true)]);
    bench
}

fn pressed(key: Key, modifiers: Modifiers) -> Event {
    key_event(key, modifiers, true)
}

#[test]
fn key_held_on_refocus_never_closes() {
    for (key, modifiers) in CLOSERS {
        let mut bench = with_the_keyboard_back();
        for repeat in 0..HELD_REPEATS {
            assert_eq!(bench.repeat_from_the_window(key, modifiers), [], "{key:?}, repeat {repeat}");
        }
        assert_eq!(bench.let_go(key, modifiers), [], "{key:?} let go");
        assert_eq!(bench.hold(key, modifiers), [Outcome::Close], "{key:?} pressed afresh");
    }
}

#[test]
fn reported_repeats_still_repeat() {
    let mut bench = with_the_keyboard_back();
    (0..NUDGES).for_each(|_| drop(bench.repeat_from_the_window(Key::CloseBracket, Modifiers::NONE)));
    assert_eq!(bench.editor.brush.size(), Brush::DEFAULT_SIZE + NUDGES as f32);
    let [from, middle, to] = across(&bench);
    bench.drag(&[from, middle]);
    bench.drag(&[middle, to]);
    let drawn = bench.editor.revision();
    assert_eq!(bench.repeat_from_the_window(Key::Z, Modifiers::COMMAND), [Outcome::Changed(drawn + 1)]);
    assert_eq!(bench.repeat_from_the_window(Key::Z, Modifiers::COMMAND), [Outcome::Changed(drawn + 2)]);
    assert!(bench.editor.job().marks.is_empty(), "both lines were taken back");
}

#[test]
fn only_the_repeat_is_forgotten() {
    let fresh_press = [pressed(Key::Escape, Modifiers::NONE), Event::PointerGone];
    let kept_repeats = [
        pressed(Key::CloseBracket, Modifiers::NONE),
        pressed(Key::Z, Modifiers::COMMAND),
        pressed(Key::W, Modifiers::NONE),
        key_event(Key::Escape, Modifiers::NONE, false),
    ];
    let mut heard = fresh_press.to_vec();
    let already_heard = heard.len();
    heard.extend(CLOSERS.map(|(key, modifiers)| pressed(key, modifiers)));
    heard.extend(kept_repeats.clone());
    forget_repeated_strikes(&mut heard, already_heard);
    assert_eq!(heard, [fresh_press.as_slice(), kept_repeats.as_slice()].concat());
    let everything = heard.clone();
    forget_repeated_strikes(&mut heard, everything.len() + 1);
    assert_eq!(heard, everything, "nothing new was heard, so nothing is forgotten");
}
