mod finishing;
mod keys;
mod picture;
mod pointer;
mod reopening;
mod unread;

use egui::{ColorImage, CursorIcon, Key, Modifiers, Pos2, vec2};

use super::super::bench::Bench;
use super::super::picture::Picture;
use super::super::tool::Tool;
use super::{WINDOW, across, bench};
use crate::annotate::composite::composite;
use crate::annotate::testing::{as_image, solid};
use crate::annotate::{Brush, Lettering, Outcome};

const GREETING: &str = "Hi";

fn with_the_text_tool() -> Bench {
    let mut bench = bench();
    assert_eq!(bench.key(Key::T, Modifiers::NONE), []);
    assert_eq!(bench.editor.tool, Tool::Text);
    bench
}

fn typing(words: &str) -> Bench {
    let mut bench = with_the_text_tool();
    let middle = bench.fit().rect().center();
    assert_eq!(bench.click(middle), []);
    assert_eq!(bench.type_text(words), []);
    bench
}

fn open(bench: &Bench) -> &Lettering {
    bench.editor.typing.as_ref().and_then(|typing| typing.typeset()).expect("an open block")
}

fn written(bench: &Bench) -> &str {
    &open(bench).words().text
}

fn caret(bench: &Bench) -> usize {
    bench.editor.typing.as_ref().expect("an open block").caret()
}

fn placed(bench: &Bench) -> Vec<Lettering> {
    bench.editor.job().marks.iter().filter_map(|mark| mark.text().cloned()).collect()
}

fn on_the_clipboard(bench: &Bench) -> ColorImage {
    let job = bench.editor.job();
    as_image(&composite(&job.base, &job.marks))
}

fn untouched(bench: &Bench) -> ColorImage {
    as_image(&bench.editor.job().base)
}

fn away_from_the_middle(bench: &Bench) -> Pos2 {
    bench.fit().rect().left_top() + vec2(30.0, 30.0)
}

fn beside_the_picture(bench: &Bench) -> Pos2 {
    let beside = bench.fit().rect().left_center() - vec2(12.0, 0.0);
    assert!(bench.layout().stage.contains(beside));
    beside
}

#[test]
fn b_t_and_control_choose_tool() {
    let mut bench = bench();
    assert_eq!(bench.editor.tool, Tool::Brush, "every window opens on the brush");
    let [brush, text] = bench.layout().tools.map(|slot| slot.center());
    assert_eq!(bench.key(Key::T, Modifiers::NONE), []);
    assert_eq!(bench.editor.tool, Tool::Text);
    assert_eq!(bench.key(Key::B, Modifiers::NONE), []);
    assert_eq!(bench.editor.tool, Tool::Brush);
    assert_eq!(bench.click(text), []);
    assert_eq!(bench.editor.tool, Tool::Text);
    assert_eq!(bench.click(text), [], "the tool already chosen stays chosen");
    assert_eq!(bench.click(brush), []);
    assert_eq!(bench.editor.tool, Tool::Brush);
    assert_eq!(bench.key(Key::T, Modifiers::COMMAND), [], "with a command key it is no shortcut");
    assert_eq!((bench.editor.tool, bench.editor.brush), (Tool::Brush, Brush::default()));
}

#[test]
fn text_tool_shows_text_cursor() {
    let mut bench = bench();
    let [from, middle, to] = across(&bench);
    let beside = beside_the_picture(&bench);
    bench.hover(middle);
    assert_eq!((bench.camera.cursor(), bench.editor.ring.target), (CursorIcon::None, 1.0));
    bench.key(Key::T, Modifiers::NONE);
    bench.hover(middle);
    assert_eq!((bench.camera.cursor(), bench.editor.ring.target), (CursorIcon::Text, 0.0));
    bench.hover(beside);
    assert_eq!((bench.camera.cursor(), bench.editor.ring.target), (CursorIcon::Default, 0.0));
    assert_eq!(bench.drag(&[from, middle, to]), [], "the pen never starts");
    assert!(bench.editor.pen.is_none() && bench.editor.job().marks.is_empty());
}

#[test]
fn brush_draws_after_text_tool() {
    let mut bench = with_the_text_tool();
    bench.key(Key::B, Modifiers::NONE);
    assert_eq!(bench.drag(&across(&bench)), [Outcome::Changed(1)]);
    assert!(bench.editor.typing.is_none() && bench.editor.job().marks[0].line().is_some());
}

#[test]
fn text_tool_finishes_the_line() {
    let mut bench = bench();
    let [from, middle, to] = across(&bench);
    bench.press(from);
    bench.hover(middle);
    assert_eq!(bench.key(Key::T, Modifiers::NONE), [Outcome::Changed(1)]);
    assert!(bench.editor.pen.is_none() && bench.editor.tool == Tool::Text);
    bench.hover(to);
    assert_eq!(bench.release(to), [], "the line ended where the tool was changed");
    assert_eq!(bench.editor.job().marks.len(), 1);
    assert!(bench.editor.typing.is_none(), "a release alone opens no block");
}

#[test]
fn click_and_typing_write_one_block() {
    let mut bench = with_the_text_tool();
    let middle = bench.fit().rect().center();
    assert_eq!(bench.click(middle), []);
    assert_eq!(bench.type_text(GREETING), []);
    assert_eq!((written(&bench), bench.editor.revision()), (GREETING, 0));
    assert_eq!(bench.key(Key::Enter, Modifiers::NONE), [Outcome::Changed(1)]);
    assert!(bench.editor.typing.is_none());
    let (job, blocks) = (bench.editor.job(), placed(&bench));
    assert_eq!((job.revision, job.marks.len(), blocks.len()), (1, 1, 1));
    assert_eq!((blocks[0].words().text.as_str(), blocks[0].color()), (GREETING, Brush::DEFAULT_COLOR));
    assert!(!blocks[0].area().is_empty());
    assert_eq!(bench.settle(), []);
}

#[test]
fn first_line_centred_on_click() {
    let mut bench = with_the_text_tool();
    let middle = bench.fit().rect().center();
    bench.click(middle);
    let clicked = bench.fit().to_image(middle);
    let frame = open(&bench).frame();
    assert_eq!((frame.left(), frame.center().y, frame.width()), (clicked.x, clicked.y, 0.0), "{frame:?}");
    assert_eq!((written(&bench), caret(&bench)), ("", 0));
}

#[test]
fn words_as_tall_as_size_at_any_zoom() {
    let letters = Brush::default().letter_size();
    let exact = typing(GREETING);
    assert_eq!((exact.fit().scale(), open(&exact).words().em), (0.5, letters * 2.0));
    let mut shrunk = Bench::new(solid(3200, 1600, [250, 250, 250]), Brush::default(), WINDOW);
    shrunk.key(Key::T, Modifiers::NONE);
    shrunk.click(shrunk.fit().rect().center());
    shrunk.type_text(GREETING);
    let em = open(&shrunk).words().em;
    assert!((em * shrunk.fit().scale() - letters).abs() < 1e-3 && em > letters * 2.0, "{em}");
}

#[test]
fn click_outside_picture_opens_nothing() {
    let mut bench = with_the_text_tool();
    let beside = beside_the_picture(&bench);
    assert_eq!(bench.click(beside), []);
    assert!(bench.editor.typing.is_none());
    assert_eq!(bench.type_text(GREETING), []);
    assert_eq!(bench.paste(GREETING), []);
    assert_eq!((bench.editor.revision(), bench.editor.job().marks.len()), (0, 0));
}

#[test]
fn oversized_screenshot_takes_no_words() {
    let mut bench = Bench::new(solid(16_385, 2, [9, 9, 9]), Brush::default(), WINDOW);
    assert!(matches!(bench.editor.picture, Picture::TooLarge(16_384)));
    bench.key(Key::T, Modifiers::NONE);
    for spot in across(&bench) {
        assert_eq!(bench.click(spot), []);
        assert!(bench.editor.typing.is_none());
    }
    assert_eq!(bench.type_text(GREETING), []);
    assert_eq!(bench.key(Key::Escape, Modifiers::NONE), [Outcome::Close]);
}

#[test]
fn blank_block_is_never_placed() {
    let mut bench = with_the_text_tool();
    let middle = bench.fit().rect().center();
    bench.click(middle);
    assert_eq!(bench.key(Key::Enter, Modifiers::NONE), [], "an empty block just closes");
    assert!(bench.editor.typing.is_none());
    bench.click(middle);
    bench.type_text("  ");
    bench.key(Key::Enter, Modifiers::SHIFT);
    bench.type_text(" ");
    assert_eq!(bench.key(Key::Escape, Modifiers::NONE), []);
    assert!(bench.editor.typing.is_none() && bench.editor.job().marks.is_empty());
    assert_eq!(bench.editor.revision(), 0);
    assert_eq!(bench.key(Key::Z, Modifiers::COMMAND), [], "nothing was placed, so nothing is undone");
}

#[test]
fn invisible_paste_places_nothing() {
    let mut bench = typing("");
    assert_eq!(bench.paste("\u{200B}\u{FEFF}\u{2060}"), []);
    assert_eq!(written(&bench), "");
    assert_eq!(bench.key(Key::Escape, Modifiers::NONE), [], "no words, so no change to report");
    assert!(bench.editor.typing.is_none() && bench.editor.job().marks.is_empty());
    assert_eq!(bench.editor.revision(), 0);
}

#[test]
fn trailing_space_left_out() {
    let mut bench = typing(GREETING);
    bench.type_text("  ");
    bench.key(Key::Enter, Modifiers::SHIFT);
    bench.key(Key::Enter, Modifiers::SHIFT);
    let tall = open(&bench).frame().height();
    assert_eq!(bench.key(Key::Enter, Modifiers::NONE), [Outcome::Changed(1)]);
    let block = &placed(&bench)[0];
    assert_eq!(block.words().text, GREETING);
    assert_eq!(block.frame().height() * 3.0, tall, "two stray lines are gone");
}
