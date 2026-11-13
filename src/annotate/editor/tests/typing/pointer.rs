use egui::{Event, Key, Modifiers, Pos2, Rect, pos2, vec2};

use super::super::super::bench::button;
use super::super::super::tool::Tool;
use super::super::IMAGE;
use super::{
    Bench, GREETING, away_from_the_middle, beside_the_picture, caret, open, placed, typing, with_the_text_tool, written,
};
use crate::annotate::{Brush, Outcome};
use crate::ui::color;

const WORD: &str = "Hello";
const PULL: [f32; 2] = [60.0, 40.0];
const NEARLY: f32 = 0.01;
const WITHIN_THE_OUTLINE: f32 = 4.0;
const PAST_THE_OUTLINE: f32 = 12.0;
const OUTLINE_IS_DRAWN: f32 = 4.0;
const UNSEEN: f32 = 20.0;

fn on_screen(bench: &Bench) -> Rect {
    let (fit, frame) = (bench.fit(), open(bench).frame());
    Rect::from_min_max(fit.to_view(frame.min), fit.to_view(frame.max))
}

fn picture() -> Rect {
    Rect::from_min_size(Pos2::ZERO, vec2(IMAGE[0] as f32, IMAGE[1] as f32))
}

fn pull(bench: &mut Bench, by: [f32; 2]) -> Vec<Outcome> {
    let from = on_screen(bench).center();
    let steps: Vec<Pos2> = (0..=4).map(|step| from + vec2(by[0], by[1]) * (step as f32 / 4.0)).collect();
    bench.drag(&steps)
}

#[test]
fn click_in_block_places_caret() {
    let mut bench = typing(WORD);
    let block = on_screen(&bench);
    assert_eq!(bench.click(block.left_center() + vec2(1.0, 0.0)), []);
    assert_eq!(caret(&bench), 0);
    assert_eq!(bench.click(block.right_center() - vec2(1.0, 0.0)), []);
    assert_eq!(caret(&bench), 5);
    assert_eq!(bench.click(block.center()), []);
    assert!((1..5).contains(&caret(&bench)), "{}", caret(&bench));
    assert_eq!((written(&bench), bench.editor.revision()), (WORD, 0));
    bench.type_text("!");
    assert_eq!(written(&bench).len(), WORD.len() + 1);
}

#[test]
fn near_press_holds_far_press_finishes() {
    let mut bench = typing(WORD);
    let block = on_screen(&bench);
    assert_eq!(bench.click(block.left_center() - vec2(WITHIN_THE_OUTLINE, 0.0)), []);
    assert_eq!((written(&bench), caret(&bench)), (WORD, 0));
    assert_eq!(bench.click(block.left_center() - vec2(PAST_THE_OUTLINE, 0.0)), [Outcome::Changed(1)]);
    assert_eq!(placed(&bench)[0].words().text, WORD);
}

#[test]
fn dragging_moves_the_open_block() {
    let mut bench = typing(WORD);
    let (before, caret_before) = (open(&bench).clone(), caret(&bench));
    assert_eq!(pull(&mut bench, PULL), []);
    let moved = vec2(PULL[0], PULL[1]) / bench.fit().scale();
    let dropped = open(&bench).clone();
    assert_eq!(dropped.words().origin, before.words().origin + moved);
    assert!(dropped.frame().min.distance(before.frame().min + moved) < NEARLY, "{:?}", dropped.frame());
    assert!((dropped.frame().size() - before.frame().size()).length() < NEARLY);
    assert_eq!((written(&bench), caret(&bench)), (WORD, caret_before), "a drag is not a click");
    assert_eq!(bench.key(Key::Enter, Modifiers::NONE), [Outcome::Changed(1)]);
    assert_eq!(placed(&bench), [dropped], "what was shown is what is placed");
}

#[test]
fn dragged_block_keeps_a_foothold() {
    for far in [[900.0, 700.0], [-900.0, -700.0], [900.0, -700.0]] {
        let mut bench = typing(WORD);
        pull(&mut bench, far);
        let block = open(&bench);
        assert!(block.frame().intersect(picture()).is_positive(), "{far:?}: {:?}", block.frame());
        assert!(!block.area().is_empty(), "{far:?}: some of its ink is still on the picture");
    }
}

#[test]
fn foothold_counts_trimmed_words() {
    let mut bench = typing(WORD);
    bench.key(Key::Enter, Modifiers::SHIFT);
    pull(&mut bench, [0.0, -700.0]);
    assert_eq!(bench.key(Key::Enter, Modifiers::NONE), [Outcome::Changed(1)]);
    let block = placed(&bench)[0].clone();
    assert_eq!(block.words().text, WORD);
    assert!(block.frame().intersect(picture()).is_positive(), "{:?}", block.frame());
    assert!(!block.area().is_empty(), "some of its ink is on the picture");
    bench.click(bench.fit().to_view(block.frame().center_bottom()) - vec2(0.0, 2.0));
    assert_eq!((written(&bench), bench.editor.revision()), (WORD, 1), "a press on what shows of it opens it again");
}

#[test]
fn edge_block_slides_as_letters_go() {
    let mut bench = typing(WORD);
    pull(&mut bench, [-900.0, 0.0]);
    (0..3).for_each(|_| drop(bench.key(Key::Backspace, Modifiers::NONE)));
    let shown = open(&bench).clone();
    assert_eq!(shown.words().text, "He");
    assert!(shown.frame().intersect(picture()).is_positive(), "while it is open: {:?}", shown.frame());
    assert!(!shown.area().is_empty(), "what is left of it can be seen");
    assert_eq!(bench.key(Key::Enter, Modifiers::NONE), [Outcome::Changed(1)]);
    assert_eq!(placed(&bench), [shown], "what was shown is what is placed");
}

#[test]
fn edge_block_starts_half_an_em_in() {
    let mut bench = with_the_text_tool();
    bench.click(bench.fit().rect().right_center() - vec2(1.0, 0.0));
    let (left, em) = (open(&bench).frame().left(), open(&bench).words().em);
    assert_eq!(left, IMAGE[0] as f32 - em / 2.0);
    bench.type_text(WORD);
    assert_eq!(open(&bench).frame().left(), left);
    assert!(!open(&bench).area().is_empty(), "the first of its letters is on the picture");
}

#[test]
fn wobbly_click_does_not_move_it() {
    let mut bench = typing(WORD);
    let before = open(&bench).frame();
    let at = on_screen(&bench).right_center() - vec2(1.0, 0.0);
    bench.press(at);
    bench.hover(at + vec2(1.5, 1.0));
    bench.release(at + vec2(1.5, 1.0));
    assert_eq!((open(&bench).frame(), caret(&bench)), (before, 5));
}

#[test]
fn released_block_stops_following() {
    let mut bench = typing(WORD);
    pull(&mut bench, PULL);
    let dropped = open(&bench).frame();
    let inside = on_screen(&bench).center();
    bench.hover(inside + vec2(80.0, 30.0));
    bench.hover(inside - vec2(80.0, 30.0));
    assert_eq!(open(&bench).frame(), dropped);
    assert_eq!(
        bench.frame(vec![button(inside, true), button(inside, false), Event::PointerMoved(inside + vec2(90.0, 0.0))]),
        []
    );
    assert_eq!(open(&bench).frame(), dropped, "a press and release within one frame holds nothing either");
}

#[test]
fn press_elsewhere_opens_another() {
    let mut bench = typing(GREETING);
    let elsewhere = away_from_the_middle(&bench);
    assert_eq!(bench.click(elsewhere), [Outcome::Changed(1)]);
    assert_eq!(placed(&bench)[0].words().text, GREETING);
    let (frame, pressed) = (open(&bench).frame(), bench.fit().to_image(elsewhere));
    assert_eq!((written(&bench), frame.left(), frame.center().y), ("", pressed.x, pressed.y));
    assert_eq!(bench.click(bench.fit().rect().center()), [], "the empty one leaves nothing behind");
    assert_eq!(bench.editor.job().marks.len(), 1);
}

#[test]
fn press_off_picture_only_finishes() {
    let mut bench = typing(GREETING);
    let beside = beside_the_picture(&bench);
    assert_eq!(bench.click(beside), [Outcome::Changed(1)]);
    assert!(bench.editor.typing.is_none());
    assert_eq!(placed(&bench).len(), 1);
}

#[test]
fn hanging_block_held_by_drawn_part() {
    let mut bench = typing(WORD);
    pull(&mut bench, [-900.0, 0.0]);
    let (block, edge) = (on_screen(&bench), bench.fit().rect().left());
    let (drawn, unseen) = (pos2(edge - OUTLINE_IS_DRAWN, block.center().y), pos2(edge - UNSEEN, block.center().y));
    assert!(block.contains(unseen) && bench.layout().stage.contains(unseen), "{block:?}");
    assert_eq!(bench.click(drawn), [], "just beside the picture the outline is drawn and holds the block");
    assert_eq!(written(&bench), WORD);
    assert_eq!(bench.click(unseen), [Outcome::Changed(1)], "further out there is nothing to see or to hold");
    assert!(bench.editor.typing.is_none());
}

#[test]
fn swatch_recolours_the_open_block() {
    let mut bench = typing(GREETING);
    let jade = bench.layout().swatches[2].center();
    let picked = Brush::default().with_color(color::GREEN);
    assert_eq!(bench.click(jade), [Outcome::Remember(picked)]);
    assert_eq!((open(&bench).color(), written(&bench)), (color::GREEN, GREETING));
    assert_eq!(bench.click(jade), []);
    assert_eq!(bench.editor.revision(), 0);
    assert_eq!(bench.key(Key::Enter, Modifiers::NONE), [Outcome::Changed(1)]);
    assert_eq!(placed(&bench)[0].color(), color::GREEN);
}

#[test]
fn slider_resizes_the_open_block() {
    let mut bench = typing(GREETING);
    let slider = bench.layout().slider;
    let (from, to) = (slider.left_center() + vec2(20.0, 0.0), slider.right_center() - vec2(20.0, 0.0));
    let (small, origin) = (open(&bench).words().em, open(&bench).words().origin);
    assert_eq!(bench.press(from), []);
    assert_eq!(bench.frame(vec![Event::PointerMoved(to)]), []);
    let (large, brush) = (open(&bench).words().em, bench.editor.brush);
    assert!(brush.size() > Brush::DEFAULT_SIZE && large > small, "{large} after {small}");
    assert_eq!(large, bench.fit().em(brush.letter_size()));
    assert_eq!((open(&bench).words().origin, written(&bench)), (origin, GREETING));
    assert_eq!(bench.release(to), [Outcome::Remember(brush)]);
    assert_eq!(bench.key(Key::Enter, Modifiers::NONE), [Outcome::Changed(1)]);
    assert_eq!(placed(&bench)[0].words().em, large);
}

#[test]
fn choosing_brush_finishes_block() {
    let mut bench = typing(GREETING);
    let [brush, text] = bench.layout().tools.map(|slot| slot.center());
    assert_eq!(bench.click(text), []);
    assert_eq!(written(&bench), GREETING);
    assert_eq!(bench.click(brush), [Outcome::Changed(1)]);
    assert!(bench.editor.typing.is_none());
    assert_eq!((bench.editor.tool, placed(&bench).len()), (Tool::Brush, 1));
}

#[test]
fn undo_wakes_with_first_words() {
    let mut bench = typing("");
    let (undo, redo) = (bench.layout().undo.center(), bench.layout().redo.center());
    assert_eq!(bench.click(undo), []);
    assert!(bench.editor.typing.is_some(), "with nothing to undo the empty block stays open");
    bench.type_text(GREETING);
    assert_eq!(bench.click(undo), [Outcome::Changed(2)]);
    assert!(bench.editor.typing.is_none() && bench.editor.job().marks.is_empty());
    assert_eq!(bench.click(redo), [Outcome::Changed(3)]);
    assert_eq!(placed(&bench)[0].words().text, GREETING);
}
