use egui::{Rect, pos2, vec2};

use super::{PLACE, proof};

const TWO_LINES: &str = "ab\ncd";
const PITCH: f32 = 63.0;

#[test]
fn caret_before_each_letter() {
    let set = proof(TWO_LINES, PLACE, 50.0);
    let frame = set.lettering().frame();
    let carets: Vec<Rect> = (0..=5).map(|index| set.caret(index)).collect();
    assert!(carets.iter().all(|caret| caret.width() == 0.0 && caret.height() == PITCH), "{carets:?}");
    assert_eq!((carets[0].left(), carets[3].left()), (40.0, 40.0));
    assert!(carets[0].left() < carets[1].left() && carets[1].left() < carets[2].left());
    assert!(carets[3].left() < carets[4].left() && carets[4].left() < carets[5].left());
    assert_eq!([0, 2, 3, 5].map(|index| carets[index].top()), [30.0, 30.0, 30.0 + PITCH, 30.0 + PITCH]);
    assert_eq!((carets[2].left(), carets[5].bottom()), (frame.right(), frame.bottom()));
    assert_eq!(set.caret(99), carets[5], "past the end is the end");
}

#[test]
fn point_finds_nearest_boundary() {
    let set = proof(TWO_LINES, PLACE, 50.0);
    for index in 0..=5 {
        let just_right_of_it = set.caret(index).center() + vec2(1.0, 0.0);
        assert_eq!(set.index_at(just_right_of_it), index);
    }
    let through_a = (set.caret(0).center().x + set.caret(1).center().x) / 2.0;
    assert_eq!((set.index_at(pos2(through_a - 2.0, 40.0)), set.index_at(pos2(through_a + 2.0, 40.0))), (0, 1));
    assert_eq!((set.index_at(pos2(900.0, 40.0)), set.index_at(pos2(-50.0, 120.0))), (2, 3));
    assert_eq!((set.index_at(pos2(60.0, -400.0)), set.index_at(pos2(60.0, 900.0))), (0, 5));
}

#[test]
fn up_and_down_change_lines() {
    let set = proof(TWO_LINES, PLACE, 50.0);
    assert_eq!([0, 1, 2].map(|index| set.index_below(index)), [3, 4, 5]);
    assert_eq!([3, 4, 5].map(|index| set.index_above(index)), [0, 1, 2]);
    assert_eq!([0, 1, 2].map(|index| set.index_above(index)), [0, 0, 0], "up from the first line is the start");
    assert_eq!([3, 4, 5].map(|index| set.index_below(index)), [5, 5, 5], "down from the last line is the end");
    let long_over_short = proof("abcdef\nxy", PLACE, 50.0);
    assert_eq!((long_over_short.index_below(6), long_over_short.index_below(0)), (9, 7));
    assert_eq!(long_over_short.index_above(9), 2, "back up to the letter boundary above the caret");
}

#[test]
fn home_and_end_stay_on_line() {
    let set = proof(TWO_LINES, PLACE, 50.0);
    assert_eq!([0, 1, 2, 3, 4, 5].map(|index| set.line_start(index)), [0, 0, 0, 3, 3, 3]);
    assert_eq!([0, 1, 2, 3, 4, 5].map(|index| set.line_end(index)), [2, 2, 2, 5, 5, 5]);
    assert_eq!((set.line_start(99), set.line_end(99)), (3, 5));
}

#[test]
fn empty_block_has_line_tall_caret() {
    let empty = proof("", PLACE, 40.0);
    let caret = Rect::from_min_max(pos2(40.0, 30.0), pos2(40.0, 80.0));
    assert_eq!((empty.caret(0), empty.caret(7), empty.lettering().frame()), (caret, caret, caret));
    assert_eq!((empty.index_at(pos2(300.0, 300.0)), empty.index_above(0), empty.index_below(0)), (0, 0, 0));
    assert_eq!((empty.line_start(0), empty.line_end(0)), (0, 0));
    let after_a_break = proof("a\n", PLACE, 40.0);
    assert_eq!(after_a_break.caret(2), caret.translate(vec2(0.0, 50.0)), "a new line starts under the first");
}

#[test]
fn caret_counts_letters_not_bytes() {
    let text = "éя😀";
    let set = proof(text, PLACE, 50.0);
    assert_eq!((text.len(), text.chars().count()), (8, 3));
    let carets = [0, 1, 2, 3].map(|index| set.caret(index).left());
    assert!(carets.windows(2).all(|pair| pair[0] < pair[1]), "{carets:?}");
    assert_eq!((carets[3], set.caret(8).left()), (set.lettering().frame().right(), carets[3]));
    assert_eq!((set.index_at(pos2(900.0, 40.0)), set.line_end(0)), (3, 3));
}
