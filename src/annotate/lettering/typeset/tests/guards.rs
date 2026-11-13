use std::sync::Arc;

use egui::{FontDefinitions, FontFamily};

use super::super::{ATLAS_SIDE, TALLEST, TALLEST_ATLAS_AT_REST, Typesetter};
use super::{IMAGE, INK, PLACE, SMALLEST_ATLAS, cramped, typesetter, words};
use crate::annotate::lettering::Proof;

const EVERY_LETTER_AND_DIGIT: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
const TENS_OF_LETTERS: [&str; 6] = ["ABCDEFGHIJ", "KLMNOPQRST", "UVWXYZabcd", "efghijklmn", "opqrstuvwx", "yz01234567"];
const WIDE_TAB: f32 = 12.0;
const TOO_SMALL_AN_ATLAS: usize = 512;
const MISSING: &str = "a font that is not there";

fn fill(typesetter: &Typesetter) -> f32 {
    typesetter.fonts.as_ref().map_or(0.0, |fonts| fonts.font_atlas_fill_ratio())
}

fn atlas_height(typesetter: &Typesetter) -> usize {
    typesetter.fonts.as_ref().map_or(0, |fonts| fonts.font_image_size()[1])
}

fn font_names(typesetter: &Typesetter) -> Vec<&str> {
    typesetter.definitions.font_data.keys().map(String::as_str).collect()
}

fn with_wide_tabs() -> FontDefinitions {
    let mut definitions = FontDefinitions::default();
    for data in definitions.font_data.values_mut() {
        Arc::make_mut(data).tweak.tab_size = WIDE_TAB;
    }
    definitions
}

#[test]
fn overfull_atlas_refuses_letters() {
    let (mut cramped, mut roomy) = (cramped(), typesetter());
    let (too_many, few) = (words(EVERY_LETTER_AND_DIGIT, PLACE, TALLEST), words("Hi", PLACE, TALLEST));
    assert!(cramped.set(&too_many, INK, IMAGE).is_none());
    assert!(cramped.fonts.is_none(), "the atlas that overflowed is thrown away");
    let after = cramped.set(&few, INK, IMAGE).map(Proof::into_lettering);
    assert!(after.is_some() && after == roomy.set(&few, INK, IMAGE).map(Proof::into_lettering));
    assert!(roomy.set(&too_many, INK, IMAGE).is_some(), "the atlas the app uses holds them all");
    assert_eq!((ATLAS_SIDE, SMALLEST_ATLAS), (4096, 1024));
}

#[test]
fn full_atlas_starts_afresh() {
    let (mut cramped, mut roomy) = (cramped(), typesetter());
    let mut fills = vec![fill(&cramped)];
    for letters in TENS_OF_LETTERS {
        let asked = words(letters, PLACE, TALLEST);
        let tight = cramped.set(&asked, INK, IMAGE).map(Proof::into_lettering);
        assert!(tight.is_some() && tight == roomy.set(&asked, INK, IMAGE).map(Proof::into_lettering), "{letters}");
        fills.push(fill(&cramped));
    }
    assert!(fills.windows(2).any(|pair| pair[1] < pair[0]), "the atlas was started afresh: {fills:?}");
    assert!(fills.iter().all(|fill| *fill < 1.0), "{fills:?}");
}

#[test]
fn atlas_holds_one_size() {
    let (mut typesetter, mut fresh) = (typesetter(), typesetter());
    assert!(typesetter.set(&words("Hello", PLACE, 100.0), INK, IMAGE).is_some());
    let one_word = fill(&typesetter);
    assert!(typesetter.set(&words("World", PLACE, 100.0), INK, IMAGE).is_some());
    assert!(fill(&typesetter) >= one_word && one_word > 0.0, "letters of the same size share the atlas");
    let larger = words("Hello", PLACE, 200.0);
    let made = typesetter.set(&larger, INK, IMAGE).map(Proof::into_lettering);
    assert!(made.is_some() && made == fresh.set(&larger, INK, IMAGE).map(Proof::into_lettering));
    assert_eq!(fill(&typesetter), fill(&fresh), "the letters of the old size are gone");
}

#[test]
fn too_wide_letter_is_refused() {
    let (wide_tabs, family) = (with_wide_tabs(), FontFamily::Proportional);
    let mut cramped = Typesetter::with_atlas(&wide_tabs, &family, SMALLEST_ATLAS);
    assert!(cramped.set(&words("a\tb", PLACE, TALLEST), INK, IMAGE).is_none());
    assert!(cramped.set(&words("a\tb", PLACE, 100.0), INK, IMAGE).is_some(), "smaller, the same tab fits");
    assert!(cramped.set(&words("a b", PLACE, TALLEST), INK, IMAGE).is_some(), "and the letters beside it always did");
    let mut roomy = Typesetter::new(&wide_tabs, &family);
    assert!(roomy.set(&words("a\tb", PLACE, TALLEST), INK, IMAGE).is_some());
}

#[test]
fn broken_fonts_refuse_not_panic() {
    let mut dangling = FontDefinitions::default();
    dangling.families.insert(FontFamily::Proportional, vec![MISSING.to_owned()]);
    let mut typesetter = Typesetter::new(&dangling, &FontFamily::Proportional);
    for _ in 0..2 {
        assert!(typesetter.set(&words("Hi", PLACE, 40.0), INK, IMAGE).is_none());
        assert!(typesetter.fonts.is_none(), "the fonts that panicked are thrown away");
    }
    let mut tiny = Typesetter::with_atlas(&FontDefinitions::default(), &FontFamily::Proportional, TOO_SMALL_AN_ATLAS);
    assert!(tiny.fonts.is_none() && tiny.set(&words("Hi", PLACE, 40.0), INK, IMAGE).is_none());
}

#[test]
fn only_chosen_family_is_loaded() {
    let proportional = typesetter();
    assert_eq!(font_names(&proportional), ["NotoEmoji-Regular", "Ubuntu-Light", "emoji-icon-font"]);
    assert_eq!(proportional.definitions.families.keys().collect::<Vec<_>>(), [&FontFamily::Proportional]);
    let monospace = Typesetter::new(&FontDefinitions::default(), &FontFamily::Monospace);
    assert_eq!(font_names(&monospace), ["Hack", "NotoEmoji-Regular", "Ubuntu-Light", "emoji-icon-font"]);
    assert_eq!(monospace.definitions.families.keys().collect::<Vec<_>>(), [&FontFamily::Monospace]);
    assert_eq!(monospace.definitions.families[&FontFamily::Monospace][0], "Hack", "the order of the fallbacks is kept");
}

#[test]
fn missing_family_falls_back() {
    let mut unknown = Typesetter::new(&FontDefinitions::default(), &FontFamily::Name("semibold".into()));
    assert_eq!(unknown.family, FontFamily::Proportional);
    let asked = words("Hi", PLACE, 40.0);
    let made = unknown.set(&asked, INK, IMAGE).map(Proof::into_lettering);
    assert!(made.is_some() && made == typesetter().set(&asked, INK, IMAGE).map(Proof::into_lettering));
}

#[test]
fn resting_lets_a_tall_atlas_go() {
    let mut typesetter = typesetter();
    assert!(typesetter.fonts.is_some(), "the fonts are loaded before the first word is set");
    assert!(typesetter.set(&words("Hi", PLACE, 40.0), INK, IMAGE).is_some());
    typesetter.rest();
    assert!(typesetter.fonts.is_some() && atlas_height(&typesetter) <= TALLEST_ATLAS_AT_REST, "a small atlas is kept");
    let large = words(EVERY_LETTER_AND_DIGIT, PLACE, TALLEST);
    let before = typesetter.set(&large, INK, IMAGE).map(Proof::into_lettering);
    assert!(before.is_some() && atlas_height(&typesetter) > TALLEST_ATLAS_AT_REST);
    typesetter.rest();
    assert!(typesetter.fonts.is_none());
    assert!(typesetter.set(&large, INK, IMAGE).map(Proof::into_lettering) == before);
}
