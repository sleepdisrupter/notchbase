use std::sync::Arc;

use egui::{Color32, Rect, pos2};

use super::super::{LEADING, SHORTEST, TALLEST};
use super::{IMAGE, INK, PLACE, set, typesetter, words};
use crate::annotate::Sheet;
use crate::annotate::composite::composite;
use crate::annotate::lettering::{Lettering, Proof};
use crate::annotate::rect::PixelRect;
use crate::annotate::testing::{as_image, solid};

const OVERHANG: f32 = 4.0;
const CLOSE: f32 = 2.0;
const SOLID: u8 = u8::MAX;
const CLEAR: u8 = 0;
const SMALL_IMAGE: [usize; 2] = [120, 90];
const WHITE: [u8; 3] = [255, 255, 255];

fn level(lettering: &Lettering, x: usize, y: usize) -> u8 {
    let area = lettering.area();
    let inside = (area.x..area.right()).contains(&x) && (area.y..area.bottom()).contains(&y);
    if inside { lettering.coverage()[(y - area.y) * area.width + x - area.x] } else { CLEAR }
}

fn count(levels: &[u8], wanted: u8) -> usize {
    levels.iter().filter(|level| **level == wanted).count()
}

#[test]
fn word_is_set_where_given() {
    let set_here = set("Hi", PLACE, 40.0);
    assert_eq!((set_here.words(), set_here.color()), (&words("Hi", PLACE, 40.0), INK));
    let (frame, ink) = (set_here.frame(), set_here.area().to_rect());
    assert_eq!((frame.min, frame.height()), (pos2(40.0, 30.0), 50.0));
    assert!(frame.width() > 20.0 && frame.width() < 80.0, "{frame:?}");
    assert!(frame.expand(OVERHANG).contains_rect(ink), "{ink:?} in {frame:?}");
    assert!(ink.height() > 20.0 && ink.height() < 40.0 && ink.width() > 20.0, "{ink:?}");
    assert_eq!(set_here.coverage().len(), set_here.area().width * set_here.area().height);
    let elsewhere = set("Hi", [140.0, 230.0], 40.0);
    assert!(elsewhere.coverage() == set_here.coverage(), "the same letters whatever the place");
    let moved = [elsewhere.area().x - set_here.area().x, elsewhere.area().y - set_here.area().y];
    assert_eq!((moved, elsewhere.area().size()), ([100, 200], set_here.area().size()));
}

#[test]
fn place_rounds_to_whole_pixel() {
    let nudged = set("Hi", [40.4, 29.6], 40.0);
    assert_eq!(nudged.words().origin, pos2(40.0, 30.0));
    assert!(nudged == set("Hi", PLACE, 40.0));
}

#[test]
fn letters_solid_inside_clear_around() {
    let letter = set("H", PLACE, 120.0);
    let (area, mask) = (letter.area(), letter.coverage());
    let row = |y: usize| &mask[y * area.width..(y + 1) * area.width];
    assert!(count(mask, SOLID) > mask.len() / 5 && count(mask, CLEAR) > mask.len() / 2);
    assert!((0..area.height).any(|y| count(row(y), SOLID) + 2 >= area.width), "the bar runs right across");
    let through_the_stems = row(area.height / 5);
    assert!(count(through_the_stems, SOLID) > 6 && count(through_the_stems, CLEAR) > area.width / 2);
    assert!(through_the_stems[1] == SOLID && through_the_stems[area.width - 2] == SOLID, "a stem on each side");
    assert_eq!(through_the_stems[area.width / 2], CLEAR, "and nothing between them above the bar");
}

#[test]
fn twice_the_size_twice_as_tall() {
    let (small, large) = (set("H", PLACE, 60.0), set("H", PLACE, 120.0));
    assert_eq!((small.frame().height(), large.frame().height()), (75.0, 150.0));
    assert!((large.frame().width() - 2.0 * small.frame().width()).abs() < CLOSE);
    let [short, tall] = [&small, &large].map(|lettering| lettering.area().height as f32);
    let [narrow, wide] = [&small, &large].map(|lettering| lettering.area().width as f32);
    assert!((tall - 2.0 * short).abs() <= CLOSE && (wide - 2.0 * narrow).abs() <= CLOSE, "{short} {tall}");
    assert!(short > 0.6 * 60.0 && short < 0.8 * 60.0, "a capital is about seven tenths of the em: {short}");
}

#[test]
fn second_line_sits_below() {
    let pitch = (80.0 * LEADING) as usize;
    let (one, two, apart) = (set("H", PLACE, 80.0), set("H\nH", PLACE, 80.0), set("H\n\nH", PLACE, 80.0));
    assert_eq!((one.frame().height(), two.frame().height(), apart.frame().height()), (100.0, 200.0, 300.0));
    assert_eq!((two.area().x, two.area().y, two.area().width), (one.area().x, one.area().y, one.area().width));
    assert_eq!((two.area().height, apart.area().height), (one.area().height + pitch, one.area().height + 2 * pitch));
    let width = one.area().width;
    assert!(two.coverage()[..one.coverage().len()] == *one.coverage(), "the first line is the single line");
    assert!(two.coverage()[pitch * width..] == *one.coverage(), "and the second is the same letter a line lower");
    assert_eq!(count(&two.coverage()[one.coverage().len()..pitch * width], CLEAR), (pitch - one.area().height) * width);
}

#[test]
fn spaces_alone_set_nothing() {
    for blank in ["", "   ", "\n", " \n  "] {
        let nothing = set(blank, PLACE, 40.0);
        assert!(nothing.area().is_empty() && nothing.coverage().is_empty() && nothing.words().is_blank(), "{blank:?}");
        assert_eq!(nothing.bounds(), Rect::NOTHING, "{blank:?}");
        assert_eq!(nothing.frame().min, pos2(40.0, 30.0), "the box of its lines is still where the caret goes");
        assert!(nothing.frame().height() >= 50.0, "{blank:?}");
    }
    assert_eq!((set("", PLACE, 40.0).frame().width(), set("\n", PLACE, 40.0).frame().height()), (0.0, 100.0));
    assert!(set("   ", PLACE, 40.0).frame().width() > 10.0);
}

#[test]
fn hanging_words_cut_at_the_edge() {
    let mut typesetter = typesetter();
    let asked = words("Hang", PLACE, 80.0);
    let whole = typesetter.set(&asked, INK, IMAGE).map(Proof::into_lettering).expect("words");
    let cut = typesetter.set(&asked, INK, SMALL_IMAGE).map(Proof::into_lettering).expect("words");
    assert_eq!(cut.area(), whole.area().intersect(PixelRect::of_image(SMALL_IMAGE)));
    assert!(cut.area() != whole.area() && cut.area().right() == 120 && cut.area().bottom() == 90);
    assert_eq!(cut.coverage().len(), cut.area().width * cut.area().height);
    assert_eq!((cut.frame(), cut.words()), (whole.frame(), whole.words()));
    for (x, y) in (0..SMALL_IMAGE[1]).flat_map(|y| (0..SMALL_IMAGE[0]).map(move |x| (x, y))) {
        assert_eq!(level(&cut, x, y), level(&whole, x, y), "{x},{y}");
    }
    let off_the_corner = typesetter.set(&words("Hang", [-60.0, -30.0], 80.0), INK, IMAGE).expect("words");
    let kept = off_the_corner.lettering();
    assert_eq!((kept.area().x, kept.area().y), (0, 0));
    for (x, y) in (0..80).flat_map(|y| (0..200).map(move |x| (x, y))) {
        assert_eq!(level(kept, x, y), level(&whole, x + 100, y + 60), "{x},{y}");
    }
    let off_the_picture = typesetter.set(&words("Hang", [5000.0, 30.0], 80.0), INK, IMAGE).expect("words");
    assert!(off_the_picture.lettering().area().is_empty() && !off_the_picture.lettering().words().is_blank());
}

#[test]
fn impossible_size_made_settable() {
    for (asked, settable) in [
        (f32::NAN, SHORTEST),
        (0.0, SHORTEST),
        (-40.0, SHORTEST),
        (1.0e9, TALLEST),
        (f32::INFINITY, TALLEST),
        (48.0, 48.0),
    ] {
        let made = set("Hi", PLACE, asked);
        assert_eq!(made.words().em, settable, "{asked}");
        assert_eq!(made.frame().height(), (settable * LEADING).round(), "{asked}");
        assert!(!made.area().is_empty(), "{asked}");
    }
    assert!(set("Hi", PLACE, 1.0e9) == set("Hi", PLACE, TALLEST));
    assert_eq!((SHORTEST, TALLEST), (6.0, 320.0));
}

#[test]
fn placeless_words_are_refused() {
    let mut typesetter = typesetter();
    for nowhere in [[f32::NAN, 30.0], [40.0, f32::INFINITY], [f32::NEG_INFINITY, f32::NAN]] {
        assert!(typesetter.set(&words("Hi", nowhere, 40.0), INK, IMAGE).is_none(), "{nowhere:?}");
    }
    assert!(typesetter.set(&words("Hi", PLACE, 40.0), INK, IMAGE).is_some());
}

#[test]
fn unknown_character_becomes_a_box() {
    let (unknown, a_box, a_question) = (set("漢", PLACE, 100.0), set("◻", PLACE, 100.0), set("?", PLACE, 100.0));
    assert!(!unknown.area().is_empty() && unknown.words().text == "漢");
    assert_eq!(unknown.area(), a_box.area());
    assert!(unknown.coverage() == a_box.coverage() && unknown.coverage() != a_question.coverage());
}

#[test]
fn same_words_same_mask() {
    let mut typesetter = typesetter();
    let asked = words("Same again,\nplease", PLACE, 36.0);
    let first = typesetter.set(&asked, INK, IMAGE).map(Proof::into_lettering).expect("words");
    assert!(typesetter.set(&words("Something else", [5.0, 300.0], 90.0), Color32::BLACK, IMAGE).is_some());
    let again = typesetter.set(&asked, INK, IMAGE).map(Proof::into_lettering).expect("words");
    assert!(first == again && first == set("Same again,\nplease", PLACE, 36.0));
    assert!(!first.area().is_empty());
}

#[test]
fn words_ink_in_the_exact_colour() {
    let base = Arc::new(solid(IMAGE[0], IMAGE[1], WHITE));
    let mut sheet = Sheet::new(base.clone()).expect("a sheet");
    let label = set("Hi", PLACE, 100.0);
    let area = label.area();
    assert_eq!(sheet.commit(label.clone()), area);
    let image = sheet.image();
    assert!(image == as_image(&composite(&base, sheet.marks())));
    for (index, level) in label.coverage().iter().enumerate().filter(|(_, level)| [SOLID, CLEAR].contains(level)) {
        let at = (area.x + index % area.width, area.y + index / area.width);
        assert_eq!(image[at], if *level == SOLID { INK } else { Color32::WHITE }, "{at:?}");
    }
    assert_eq!(sheet.undo(), Some(area));
    assert!(sheet.image() == as_image(&base));
}
