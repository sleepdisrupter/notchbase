use egui::{Event, Key, Modifiers, pos2, vec2};

use super::super::bench::Bench;
use super::super::status::Reading;
use super::{across, bench};
use crate::annotate::{Brush, Outcome};
use crate::ui::color;

#[test]
fn swatch_click_remembered_once() {
    let mut bench = bench();
    let jade = bench.layout().swatches[2].center();
    let outcomes = bench.click(jade);
    let picked = Brush::default().with_color(color::GREEN);
    assert_eq!(outcomes, [Outcome::Remember(picked)]);
    assert_eq!(bench.editor.brush, picked);
    assert_eq!(bench.click(jade), [], "the brush that is already remembered is not remembered again");
    assert_eq!(bench.settle(), []);
}

#[test]
fn digits_pick_brackets_nudge() {
    let mut bench = bench();
    assert_eq!(bench.key(Key::Num5, Modifiers::NONE), [Outcome::Remember(Brush::default().with_color(color::INK))]);
    let wider = Brush::new(color::INK, Brush::DEFAULT_SIZE + 1.0);
    assert_eq!(bench.key(Key::CloseBracket, Modifiers::NONE), [Outcome::Remember(wider)]);
    bench.key(Key::OpenBracket, Modifiers::NONE);
    bench.key(Key::OpenBracket, Modifiers::NONE);
    assert_eq!(bench.editor.brush, Brush::new(color::INK, Brush::DEFAULT_SIZE - 1.0));
    (0..40).for_each(|_| drop(bench.key(Key::OpenBracket, Modifiers::NONE)));
    assert_eq!(bench.editor.brush.size(), Brush::THINNEST);
    assert_eq!(bench.key(Key::OpenBracket, Modifiers::NONE), [], "the thinnest brush has nothing new to remember");
}

#[test]
fn slider_remembered_on_release() {
    let mut bench = bench();
    let slider = bench.layout().slider;
    let (from, to) = (slider.left_center() + vec2(20.0, 0.0), slider.right_center() - vec2(20.0, 0.0));
    assert_eq!(bench.press(from), []);
    let mut seen = Vec::new();
    for step in 0..=10 {
        let at = from + (to - from) * (step as f32 / 10.0);
        assert_eq!(bench.frame(vec![Event::PointerMoved(at)]), [], "step {step}");
        seen.push(bench.editor.brush.size());
    }
    assert!(seen.windows(2).all(|pair| pair[0] <= pair[1]) && seen[0] < seen[10], "{seen:?}");
    let outcomes = bench.release(to);
    assert_eq!(outcomes, [Outcome::Remember(bench.editor.brush)]);
    assert_eq!(bench.editor.brush.size(), seen[10]);
    assert_eq!(bench.editor.brush.color(), Brush::DEFAULT_COLOR);
}

#[test]
fn undo_redo_keys_walk_history() {
    let mut bench = bench();
    bench.drag(&across(&bench));
    assert_eq!(bench.key(Key::Z, Modifiers::COMMAND), [Outcome::Changed(2)]);
    assert_eq!(bench.editor.job().marks.len(), 0);
    assert_eq!(bench.key(Key::Z, Modifiers::COMMAND), [], "nothing is left to undo");
    assert_eq!(bench.key(Key::Z, Modifiers::COMMAND | Modifiers::SHIFT), [Outcome::Changed(3)]);
    assert_eq!(bench.editor.job().marks.len(), 1);
    assert_eq!(bench.key(Key::Z, Modifiers::COMMAND | Modifiers::SHIFT), []);
}

#[test]
fn undo_redo_buttons_need_history() {
    let mut bench = bench();
    let (undo, redo) = (bench.layout().undo.center(), bench.layout().redo.center());
    assert_eq!(bench.click(undo), []);
    bench.drag(&across(&bench));
    assert_eq!(bench.click(redo), []);
    assert_eq!(bench.click(undo), [Outcome::Changed(2)]);
    assert_eq!(bench.click(redo), [Outcome::Changed(3)]);
}

#[test]
fn every_way_out_asks_to_close() {
    let mut bench = bench();
    assert_eq!(bench.key(Key::Escape, Modifiers::NONE), [Outcome::Close]);
    assert_eq!(bench.key(Key::Enter, Modifiers::NONE), [Outcome::Close]);
    assert_eq!(bench.key(Key::W, Modifiers::COMMAND), [Outcome::Close]);
    assert_eq!(bench.key(Key::W, Modifiers::NONE), []);
    let done = bench.layout().done.center();
    assert_eq!(bench.click(done), [Outcome::Close]);
}

#[test]
fn status_copied_only_when_current() {
    let mut bench = bench();
    let reading = |bench: &Bench| bench.editor.status.reading(bench.editor.job().revision);
    assert_eq!(reading(&bench), None);
    bench.drag(&across(&bench));
    assert_eq!(reading(&bench), Some(Reading::Copying));
    bench.editor.copied(1);
    assert_eq!(reading(&bench), Some(Reading::Copied));
    assert!(bench.editor.is_animating(), "the status pops as the copy lands");
    bench.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(reading(&bench), Some(Reading::Copying));
    bench.editor.copied(1);
    assert_eq!(reading(&bench), Some(Reading::Copying), "a stale copy is not the picture on screen");
    bench.editor.not_copied(2);
    assert_eq!(bench.settle(), [], "admitting it is painted without a stir");
    assert_eq!(reading(&bench), Some(Reading::NotCopied));
    bench.editor.copied(2);
    assert_eq!(reading(&bench), Some(Reading::Copied));
}

#[test]
fn failed_copy_blames_its_picture() {
    let mut bench = bench();
    let reading = |bench: &Bench| bench.editor.status.reading(bench.editor.revision());
    bench.drag(&across(&bench));
    bench.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(bench.editor.revision(), 2);
    bench.editor.not_copied(1);
    assert_eq!(reading(&bench), Some(Reading::Copying), "the picture on screen has not been tried yet");
    bench.editor.not_copied(2);
    assert_eq!(reading(&bench), Some(Reading::NotCopied));
}

#[test]
fn window_rests_until_there_is_cause() {
    let mut bench = bench();
    assert!(!bench.editor.is_animating(), "the toolbar has risen");
    bench.hover(bench.fit().rect().center());
    assert!(bench.editor.is_animating(), "the brush ring fades in over the picture");
    bench.settle();
    assert!(!bench.editor.is_animating());
    bench.click(bench.layout().swatches[4].center());
    assert!(bench.editor.is_animating(), "the ring glides to the picked swatch");
    bench.settle();
    assert!(!bench.editor.is_animating());
}

#[test]
fn title_strip_moves_the_window() {
    let bench = bench();
    assert!(bench.editor.moves_the_window_from(pos2(450.0, 30.0)));
    assert!(!bench.editor.moves_the_window_from(bench.fit().rect().center()));
    assert!(!bench.editor.moves_the_window_from(bench.layout().toolbar.center()));
}
