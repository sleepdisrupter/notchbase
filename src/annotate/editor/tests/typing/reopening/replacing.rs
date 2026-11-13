use egui::{Event, Key, Modifiers, pos2, vec2};

use super::super::super::super::status::Reading;
use super::super::super::{across, shown};
use super::super::beside_the_picture;
use super::{
    Bench, EDITED, PULL, REDO, WORD, after_the_last_letter, first_block, on_the_clipboard, open, placed, pull,
    reopened_and_edited, replaced, untouched, with_a_placed_block, written,
};
use crate::annotate::composite::composite;
use crate::annotate::testing::as_image;
use crate::annotate::{Brush, Outcome};
use crate::ui::color;

type Finish = fn(&mut Bench) -> Vec<Outcome>;

const FINISHES: [(&str, Finish); 8] = [
    ("enter", |bench| bench.key(Key::Enter, Modifiers::NONE)),
    ("escape", |bench| bench.key(Key::Escape, Modifiers::NONE)),
    ("the close shortcut", |bench| bench.hold(Key::W, Modifiers::COMMAND)),
    ("a press beside the picture", |bench| bench.click(beside_the_picture(bench))),
    ("choosing the brush", |bench| bench.click(bench.layout().tools[0].center())),
    ("losing the keyboard", |bench| bench.frame(vec![Event::WindowFocused(false)])),
    ("the window finishing what is in progress", |bench| bench.editor.finish_what_is_in_progress(&mut Vec::new())),
    ("the window closing", |bench| bench.editor.close()),
];

fn reports_a_change(outcomes: &[Outcome]) -> bool {
    outcomes.iter().any(|outcome| matches!(outcome, Outcome::Changed(_)))
}

fn without_the_block_at(bench: &Bench, lifted: usize) -> egui::ColorImage {
    let job = bench.editor.job();
    let rest = job.marks.iter().enumerate().filter(|(at, _)| *at != lifted).map(|(_, mark)| mark.clone());
    as_image(&composite(&job.base, &rest.collect::<Vec<_>>()))
}

#[test]
fn edited_block_replaced_in_place() {
    for (way, finish) in FINISHES {
        let mut bench = reopened_and_edited();
        let outcomes = finish(&mut bench);
        assert_eq!(outcomes.first(), Some(&Outcome::Changed(2)), "{way}");
        assert!(bench.editor.typing.is_none(), "{way}");
        assert_eq!((bench.editor.job().marks.len(), placed(&bench)[0].words().text.as_str()), (1, EDITED), "{way}");
        bench.frame(Vec::new());
        assert!(shown(&bench) == on_the_clipboard(&bench), "{way}");
    }
}

#[test]
fn untouched_block_goes_back_as_is() {
    for (way, finish) in FINISHES {
        let mut bench = with_a_placed_block(WORD);
        let with_words = shown(&bench);
        bench.click(first_block(&bench).center());
        assert!(!reports_a_change(&finish(&mut bench)), "{way}");
        assert!(bench.editor.typing.is_none(), "{way}");
        bench.frame(Vec::new());
        assert_eq!(bench.editor.revision(), 1, "{way}");
        assert!(shown(&bench) == with_words && on_the_clipboard(&bench) == with_words, "{way}");
    }
}

#[test]
fn trailing_space_is_no_change() {
    let mut bench = with_a_placed_block(WORD);
    bench.editor.copied(1);
    bench.click(after_the_last_letter(&bench));
    bench.type_text("  ");
    bench.key(Key::Enter, Modifiers::SHIFT);
    assert_eq!(bench.editor.reading(), Some(Reading::Copied), "what will be trimmed away is no change to wait for");
    assert_eq!(bench.key(Key::Enter, Modifiers::NONE), [], "trimmed, it reads as it did");
    assert_eq!((bench.editor.revision(), placed(&bench)[0].words().text.as_str()), (1, WORD));
}

#[test]
fn reopened_keeps_colour_until_swatch() {
    let mut bench = with_a_placed_block(WORD);
    let jade = bench.layout().swatches[2].center();
    assert_eq!(bench.click(jade), [Outcome::Remember(Brush::default().with_color(color::GREEN))]);
    bench.click(after_the_last_letter(&bench));
    bench.type_text("!");
    assert_eq!(open(&bench).color(), Brush::DEFAULT_COLOR, "typing leaves the colour it was placed in");
    let em = open(&bench).words().em;
    assert_eq!(bench.click(jade), [], "the brush is already jade, so nothing new is remembered");
    assert_eq!((open(&bench).color(), open(&bench).words().em), (color::GREEN, em), "the size is its own still");
    assert_eq!(bench.key(Key::Enter, Modifiers::NONE), [Outcome::Changed(2)]);
    assert_eq!((placed(&bench)[0].color(), placed(&bench)[0].words().text.as_str()), (color::GREEN, EDITED));
}

#[test]
fn reopened_keeps_size_until_slider() {
    let mut bench = with_a_placed_block(WORD);
    let placed_at = placed(&bench)[0].words().em;
    let on_the_slider = bench.layout().slider.right_center() - vec2(24.0, 0.0);
    bench.click(on_the_slider);
    let thick = bench.editor.brush;
    assert!(thick.size() > Brush::DEFAULT_SIZE);
    bench.click(first_block(&bench).center());
    assert_eq!(open(&bench).words().em, placed_at, "the brush grew, the placed words did not");
    assert_eq!(bench.press(on_the_slider), [], "the knob does not move, so nothing new is remembered");
    assert_eq!((bench.editor.brush, replaced(&bench)), (thick, Some(0)));
    assert_eq!(open(&bench).words().em, bench.fit().em(thick.letter_size()));
    assert_eq!(open(&bench).color(), Brush::DEFAULT_COLOR, "the colour is its own still");
    bench.release(on_the_slider);
    assert_eq!(bench.key(Key::Enter, Modifiers::NONE), [Outcome::Changed(2)]);
    assert!(placed(&bench)[0].words().em > placed_at);
}

#[test]
fn status_holds_until_block_differs() {
    let mut bench = with_a_placed_block(WORD);
    bench.editor.copied(1);
    bench.click(after_the_last_letter(&bench));
    assert_eq!(bench.editor.reading(), Some(Reading::Copied), "the window still shows what was copied");
    bench.type_text("!");
    assert_eq!(bench.editor.reading(), None, "the clipboard does not hold this edit");
    bench.key(Key::Backspace, Modifiers::NONE);
    assert_eq!(bench.editor.reading(), Some(Reading::Copied));
    (0..WORD.len()).for_each(|_| drop(bench.key(Key::Backspace, Modifiers::NONE)));
    assert_eq!((written(&bench), bench.editor.reading()), ("", None), "an emptied block is a change too");
    assert_eq!(bench.key(Key::Enter, Modifiers::NONE), [Outcome::Changed(2)]);
    assert_eq!(bench.editor.reading(), Some(Reading::Copying));
}

#[test]
fn dragged_block_counts_before_release() {
    let mut bench = with_a_placed_block(WORD);
    bench.editor.copied(1);
    let from = first_block(&bench).center();
    bench.press(from);
    assert_eq!(bench.editor.reading(), Some(Reading::Copied));
    bench.hover(from + vec2(PULL[0], PULL[1]));
    assert_eq!(bench.editor.reading(), None);
}

#[test]
fn screen_matches_clipboard_on_replace() {
    let mut bench = super::super::super::bench();
    let [from, middle, to] = across(&bench);
    bench.drag(&[from, middle, to]);
    bench.key(Key::T, Modifiers::NONE);
    bench.click(middle);
    bench.type_text(WORD);
    bench.key(Key::Enter, Modifiers::NONE);
    bench.key(Key::Num3, Modifiers::NONE);
    bench.key(Key::B, Modifiers::NONE);
    bench.drag(&[pos2(to.x, from.y), middle, pos2(from.x, to.y)]);
    assert_eq!((bench.editor.job().marks.len(), bench.editor.revision()), (3, 3));
    bench.key(Key::T, Modifiers::NONE);
    let block = first_block(&bench);
    bench.click(block.right_center() - vec2(1.0, 0.0));
    assert!(shown(&bench) == without_the_block_at(&bench, 1), "lifted, the block is off the picture underneath");
    bench.type_text(" there");
    assert_eq!(bench.key(Key::Enter, Modifiers::NONE), [Outcome::Changed(4)]);
    let edited = shown(&bench);
    assert!(edited == on_the_clipboard(&bench));
    let held = first_block(&bench).center();
    pull(&mut bench, held, [-70.0, 50.0]);
    assert!(shown(&bench) == without_the_block_at(&bench, 1), "lifted again to be moved");
    assert_eq!(bench.key(Key::Escape, Modifiers::NONE), [Outcome::Changed(5)]);
    let moved = shown(&bench);
    assert!(moved == on_the_clipboard(&bench) && moved != edited);
    assert_eq!(bench.editor.job().marks.len(), 3, "replacing never adds a mark");
    bench.key(Key::Z, Modifiers::COMMAND);
    assert!(shown(&bench) == edited && shown(&bench) == on_the_clipboard(&bench), "the move is undone");
    for step in 0..4 {
        bench.key(Key::Z, Modifiers::COMMAND);
        assert!(shown(&bench) == on_the_clipboard(&bench), "undo {step}");
    }
    assert!(shown(&bench) == untouched(&bench) && bench.editor.job().marks.is_empty());
    for step in 0..5 {
        bench.key(Key::Z, REDO);
        assert!(shown(&bench) == on_the_clipboard(&bench), "redo {step}");
    }
    assert!(shown(&bench) == moved, "everything is back, the replacements included");
}
