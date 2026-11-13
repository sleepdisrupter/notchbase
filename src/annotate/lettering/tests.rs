use egui::{Color32, Pos2, Rect, pos2};

use crate::annotate::lettering::{Lettering, Words};
use crate::annotate::rect::PixelRect;
use crate::annotate::testing::{block, lettering};
use crate::ui::color;

fn area(x: usize, y: usize, width: usize, height: usize) -> PixelRect {
    PixelRect { x, y, width, height }
}

#[test]
fn lettering_is_always_opaque() {
    let glassy = Color32::from_rgba_premultiplied(40, 30, 20, 60);
    assert_eq!(block(glassy, area(2, 2, 4, 4)).color(), Color32::from_rgb(40, 30, 20));
}

#[test]
fn keeps_its_words_and_mask() {
    let set = lettering(color::RED, area(10, 20, 3, 2), vec![0, 128, 255, 255, 128, 0]);
    assert_eq!((set.area(), set.coverage()), (area(10, 20, 3, 2), &[0, 128, 255, 255, 128, 0][..]));
    assert_eq!(set.bounds(), Rect::from_min_max(pos2(10.0, 20.0), pos2(13.0, 22.0)));
    assert_eq!(set.words().origin, pos2(10.0, 20.0));
    assert!(!set.words().is_blank());
}

#[test]
fn click_tested_against_line_box() {
    let set = block(color::RED, area(10, 20, 30, 12));
    assert_eq!(set.frame(), Rect::from_min_max(pos2(10.0, 20.0), pos2(40.0, 32.0)));
    assert!(set.is_at(pos2(11.0, 21.0)) && set.is_at(pos2(39.5, 31.5)));
    assert!(!set.is_at(pos2(9.0, 21.0)) && !set.is_at(pos2(20.0, 33.0)));
}

#[test]
fn empty_lettering_is_nothing() {
    let empty = Lettering::empty();
    assert!(empty.area().is_empty() && empty.coverage().is_empty() && empty.words().is_blank());
    assert_eq!(empty.bounds(), Rect::NOTHING);
    assert!(!empty.is_at(Pos2::ZERO) && !empty.is_at(pos2(40.0, 40.0)));
}

#[test]
fn spaces_and_breaks_are_blank() {
    let words = |text: &str| Words { text: text.to_owned(), origin: Pos2::ZERO, em: 20.0 };
    assert!(words("").is_blank() && words("  ").is_blank() && words(" \n\t \n").is_blank());
    assert!(!words(" a ").is_blank() && !words("\n.\n").is_blank());
    let mut spaces = block(color::RED, area(0, 0, 8, 8));
    spaces.words.text = "   ".to_owned();
    assert!(!spaces.is_at(pos2(4.0, 4.0)), "a block that holds no words is never opened by a click");
}
