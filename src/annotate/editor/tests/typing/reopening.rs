mod replacing;

use egui::{Key, Modifiers, Pos2, Rect, vec2};

use super::super::shown;
use super::{Bench, caret, on_the_clipboard, open, placed, typing, untouched, written};
use crate::annotate::mark::Mark;
use crate::annotate::{Brush, Lettering, Outcome};
use crate::ui::color;

const WORD: &str = "Hello";
const EDITED: &str = "Hello!";
const PULL: [f32; 2] = [60.0, 40.0];
const REDO: Modifiers = Modifiers::COMMAND.plus(Modifiers::SHIFT);
const HELD_REPEATS: usize = 12;

fn with_a_placed_block(words: &str) -> Bench {
    let mut bench = typing(words);
    assert_eq!(bench.key(Key::Enter, Modifiers::NONE), [Outcome::Changed(1)]);
    bench
}

fn seen(bench: &Bench, block: &Lettering) -> Rect {
    let (fit, frame) = (bench.fit(), block.frame());
    Rect::from_min_max(fit.to_view(frame.min), fit.to_view(frame.max))
}

fn first_block(bench: &Bench) -> Rect {
    seen(bench, &placed(bench)[0])
}

fn after_the_last_letter(bench: &Bench) -> Pos2 {
    first_block(bench).right_center() - vec2(1.0, 0.0)
}

fn replaced(bench: &Bench) -> Option<usize> {
    bench.editor.typing.as_ref().and_then(|typing| typing.replaces())
}

fn pull(bench: &mut Bench, from: Pos2, by: [f32; 2]) -> Vec<Outcome> {
    let steps: Vec<Pos2> = (0..=4).map(|step| from + vec2(by[0], by[1]) * (step as f32 / 4.0)).collect();
    bench.drag(&steps)
}

fn reopened_and_edited() -> Bench {
    let mut bench = with_a_placed_block(WORD);
    assert_eq!(bench.click(after_the_last_letter(&bench)), []);
    assert_eq!(bench.type_text("!"), []);
    assert_eq!((written(&bench), replaced(&bench)), (EDITED, Some(0)));
    bench
}

#[test]
fn click_reopens_a_placed_block() {
    let mut bench = with_a_placed_block(WORD);
    let was = placed(&bench)[0].clone();
    bench.key(Key::Num3, Modifiers::NONE);
    (0..6).for_each(|_| drop(bench.key(Key::CloseBracket, Modifiers::NONE)));
    assert_eq!(bench.click(first_block(&bench).center()), [], "opening a block is not a change");
    assert_eq!(replaced(&bench), Some(0));
    assert_eq!(open(&bench), &was, "the very words, colour, size and place it was given");
    assert!((1..5).contains(&caret(&bench)), "the caret goes where the click was: {}", caret(&bench));
    assert_eq!((bench.editor.revision(), bench.editor.job().marks.len()), (1, 1));
    assert_eq!(bench.settle(), []);
    assert!(!bench.editor.is_animating(), "an open block asks for no frames");
}

#[test]
fn reopened_block_drawn_once() {
    let mut bench = with_a_placed_block(WORD);
    let with_words = shown(&bench);
    assert!(with_words != untouched(&bench));
    bench.click(first_block(&bench).center());
    assert!(shown(&bench) == untouched(&bench), "the picture underneath is drawn without the block");
    assert!(on_the_clipboard(&bench) == with_words, "which the clipboard still holds");
    assert_eq!(bench.key(Key::Enter, Modifiers::NONE), [], "put back as it was, it is no change");
    assert!(bench.editor.typing.is_none() && shown(&bench) == with_words);
    assert_eq!((bench.editor.revision(), bench.editor.job().marks.len()), (1, 1));
    assert_eq!(bench.key(Key::Z, Modifiers::COMMAND), [Outcome::Changed(2)]);
    assert!(bench.editor.job().marks.is_empty(), "no step was added by opening and closing it");
}

#[test]
fn brush_draws_over_placed_words() {
    let mut bench = with_a_placed_block(WORD);
    let block = first_block(&bench);
    bench.key(Key::B, Modifiers::NONE);
    assert_eq!(bench.drag(&[block.left_center(), block.center(), block.right_center()]), [Outcome::Changed(2)]);
    assert!(bench.editor.typing.is_none());
    assert_eq!((bench.editor.job().marks.len(), placed(&bench).len()), (2, 1));
}

#[test]
fn edited_block_stays_under_lines() {
    let mut bench = with_a_placed_block(WORD);
    let block = first_block(&bench);
    bench.key(Key::Num3, Modifiers::NONE);
    bench.key(Key::B, Modifiers::NONE);
    let across = [block.left_center() - vec2(30.0, 0.0), block.center(), block.right_center() + vec2(30.0, 0.0)];
    assert_eq!(bench.drag(&across), [Outcome::Changed(2)]);
    bench.key(Key::T, Modifiers::NONE);
    assert_eq!(bench.click(after_the_last_letter(&bench)), [], "the line over the words does not keep a click away");
    assert_eq!(bench.type_text("!"), []);
    assert_eq!(bench.key(Key::Enter, Modifiers::NONE), [Outcome::Changed(3)]);
    let marks = bench.editor.job().marks;
    assert_eq!(marks.len(), 2, "the block was replaced where it stood, not added again");
    assert_eq!(marks[0].text().map(|block| block.words().text.as_str()), Some(EDITED));
    assert_eq!(marks[0].text().map(Lettering::color), Some(Brush::DEFAULT_COLOR), "typing does not recolour it");
    assert!(marks[1].line().is_some(), "the line is still the mark above it");
    let crossing = bench.fit().to_image(block.center());
    assert_eq!(shown(&bench)[(crossing.x as usize, crossing.y as usize)], color::GREEN);
    assert!(shown(&bench) == on_the_clipboard(&bench));
}

#[test]
fn undo_old_words_redo_new() {
    let mut bench = reopened_and_edited();
    let old = bench.editor.job().marks[0].text().cloned().expect("the placed block");
    assert_eq!(bench.key(Key::Enter, Modifiers::NONE), [Outcome::Changed(2)]);
    let new = placed(&bench)[0].clone();
    assert_eq!((old.words().text.as_str(), new.words().text.as_str()), (WORD, EDITED));
    assert_eq!(bench.key(Key::Z, Modifiers::COMMAND), [Outcome::Changed(3)]);
    assert_eq!(placed(&bench), [old]);
    assert!(shown(&bench) == on_the_clipboard(&bench));
    assert_eq!(bench.key(Key::Z, REDO), [Outcome::Changed(4)]);
    assert_eq!(placed(&bench), [new]);
    assert!(shown(&bench) == on_the_clipboard(&bench));
    bench.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(bench.key(Key::Z, Modifiers::COMMAND), [Outcome::Changed(6)]);
    assert!(bench.editor.job().marks.is_empty() && shown(&bench) == untouched(&bench));
}

#[test]
fn undo_takes_back_open_edit() {
    let mut bench = reopened_and_edited();
    assert_eq!(bench.key(Key::Z, Modifiers::COMMAND), [Outcome::Changed(3)]);
    assert!(bench.editor.typing.is_none());
    assert_eq!(placed(&bench)[0].words().text, WORD, "the words it had before it was opened");
    assert_eq!(bench.key(Key::Z, REDO), [Outcome::Changed(4)]);
    assert_eq!(placed(&bench)[0].words().text, EDITED, "and the edit is not lost");
}

#[test]
fn drag_moves_a_placed_block() {
    let mut bench = with_a_placed_block(WORD);
    let was = placed(&bench)[0].clone();
    let from = first_block(&bench).center();
    assert_eq!(pull(&mut bench, from, PULL), [], "it is moved while open and placed when finished");
    let moved = vec2(PULL[0], PULL[1]) / bench.fit().scale();
    assert_eq!((replaced(&bench), open(&bench).words().origin), (Some(0), was.words().origin + moved));
    assert_eq!((written(&bench), caret(&bench)), (WORD, 5), "a drag is not a click");
    assert_eq!(bench.key(Key::Enter, Modifiers::NONE), [Outcome::Changed(2)]);
    let now = placed(&bench);
    assert_eq!((now.len(), bench.editor.job().marks.len()), (1, 1));
    assert_eq!((now[0].words().origin, now[0].words().em), (was.words().origin + moved, was.words().em));
    assert_eq!((now[0].words().text.as_str(), now[0].color()), (WORD, was.color()));
    assert!(shown(&bench) == on_the_clipboard(&bench));
    assert_eq!(bench.key(Key::Z, Modifiers::COMMAND), [Outcome::Changed(3)]);
    assert_eq!(placed(&bench), [was], "undo puts it back where it was");
}

#[test]
fn old_place_of_moved_block_is_empty() {
    let mut bench = with_a_placed_block(WORD);
    let old_place = first_block(&bench).center();
    pull(&mut bench, old_place, [0.0, 60.0]);
    assert_eq!(bench.click(old_place), [Outcome::Changed(2)], "the move is placed first");
    assert_eq!((replaced(&bench), written(&bench)), (None, ""), "and the press opens a new block, not the old words");
    assert_eq!((placed(&bench).len(), bench.editor.job().marks.len()), (1, 1));
    assert!(!first_block(&bench).contains(old_place));
}

#[test]
fn topmost_block_is_opened() {
    let mut bench = with_a_placed_block(WORD);
    let under = first_block(&bench);
    bench.click(under.left_center() - vec2(40.0, 0.0));
    bench.type_text("Wide words on top");
    assert_eq!(bench.key(Key::Enter, Modifiers::NONE), [Outcome::Changed(2)]);
    let over = seen(&bench, &placed(&bench)[1]);
    assert!(over.contains(under.center()), "{over:?} lies over {under:?}");
    bench.click(under.center());
    assert_eq!((replaced(&bench), written(&bench)), (Some(1), "Wide words on top"));
}

#[test]
fn emptied_block_leaves_a_place() {
    let mut bench = with_a_placed_block("Hi");
    let (was, inside) = (placed(&bench)[0].clone(), first_block(&bench).center());
    bench.click(after_the_last_letter(&bench));
    (0..2).for_each(|_| drop(bench.key(Key::Backspace, Modifiers::NONE)));
    assert_eq!((written(&bench), replaced(&bench)), ("", Some(0)));
    assert_eq!(bench.key(Key::Backspace, Modifiers::NONE), [Outcome::Changed(2)], "erasing once more drops it");
    assert!(bench.editor.typing.is_none());
    assert_eq!(bench.editor.job().marks, [Mark::from(Lettering::empty())], "its place in the order is kept");
    assert!(shown(&bench) == untouched(&bench) && shown(&bench) == on_the_clipboard(&bench));
    bench.click(inside);
    assert_eq!((replaced(&bench), written(&bench)), (None, ""), "a block with no words is never opened again");
    assert_eq!(bench.key(Key::Escape, Modifiers::NONE), []);
    assert_eq!(bench.key(Key::Z, Modifiers::COMMAND), [Outcome::Changed(3)]);
    assert_eq!(placed(&bench), [was]);
    assert!(shown(&bench) == on_the_clipboard(&bench) && shown(&bench) != untouched(&bench));
}

#[test]
fn held_backspace_spares_placed_words() {
    let mut bench = with_a_placed_block("Hi");
    let was = placed(&bench)[0].clone();
    bench.click(after_the_last_letter(&bench));
    for repeat in 0..HELD_REPEATS {
        assert_eq!(bench.hold(Key::Backspace, Modifiers::NONE), [], "repeat {repeat} reports no change");
    }
    assert_eq!((written(&bench), replaced(&bench)), ("", Some(0)), "the emptied block is still open");
    assert_eq!((bench.editor.revision(), placed(&bench)), (1, vec![was]));
    bench.let_go(Key::Backspace, Modifiers::NONE);
    assert_eq!(bench.key(Key::Backspace, Modifiers::NONE), [Outcome::Changed(2)], "only a new press drops it");
    assert!(bench.editor.typing.is_none());
}

#[test]
fn blank_reopened_block_is_emptied() {
    let mut bench = with_a_placed_block("Hi");
    bench.click(after_the_last_letter(&bench));
    (0..2).for_each(|_| drop(bench.key(Key::Backspace, Modifiers::NONE)));
    bench.type_text("  ");
    assert_eq!(bench.key(Key::Enter, Modifiers::NONE), [Outcome::Changed(2)]);
    assert_eq!(bench.editor.job().marks, [Mark::from(Lettering::empty())]);
    assert!(shown(&bench) == untouched(&bench));
}
