use std::sync::Arc;

use egui::{FontDefinitions, FontFamily, Vec2, pos2, vec2};

use super::Typesetter;
use crate::annotate::Sheet;
use crate::annotate::lettering::{Proof, Words};
use crate::annotate::sheet::timing::{FIVE_K, LAPTOP, PAPER, quickest, timed};
use crate::annotate::testing::solid;
use crate::ui::snapshot::Camera;
use crate::ui::{color, font};

const SENTENCE: &str = "The quick brown fox jumps over the lazy dog, 42 times.";
const TWO_LINES: &str = "The quick brown fox jumps\nover the lazy dog, 42 times.";
const SIZES: [f32; 3] = [60.0, 200.0, 320.0];
const CORNER: [f32; 2] = [200.0, 300.0];
const SCRATCH_WINDOW: Vec2 = vec2(64.0, 64.0);
const CASES: [(&str, [usize; 2]); 2] = [("a laptop screenshot", LAPTOP), ("a 5K screenshot", FIVE_K)];

fn the_fonts_of_the_app() -> (FontDefinitions, FontFamily) {
    let mut camera = Camera::new(SCRATCH_WINDOW, 1.0);
    let mut definitions = FontDefinitions::empty();
    camera.pass(Vec::new(), |ui| definitions = ui.ctx().fonts(|fonts| fonts.definitions().clone()));
    (definitions, font::semibold(1.0).family)
}

fn atlas(typesetter: &Typesetter) -> [usize; 2] {
    typesetter.fonts.as_ref().map_or([0, 0], |fonts| fonts.font_image_size())
}

#[test]
#[ignore = "prints how long words take to set, commit and undo; run it by name with --release"]
fn words_on_large_screenshots_are_timed() {
    let (definitions, family) = the_fonts_of_the_app();
    let ready = quickest(|| timed(|| Typesetter::new(&definitions, &family)));
    println!("a typesetter with the app's {family} letters is ready in {ready:.1} ms");
    for (name, image) in CASES {
        let base = Arc::new(solid(image[0], image[1], PAPER));
        for (em, text) in SIZES.into_iter().flat_map(|em| [(em, SENTENCE), (em, TWO_LINES)]) {
            let asked = Words { text: text.to_owned(), origin: pos2(CORNER[0], CORNER[1]), em };
            let first = quickest(|| {
                let mut typesetter = Typesetter::new(&definitions, &family);
                timed(|| typesetter.set(&asked, color::INK, image))
            });
            let mut typesetter = Typesetter::new(&definitions, &family);
            let again = quickest(|| timed(|| typesetter.set(&asked, color::INK, image)));
            let label = typesetter.set(&asked, color::INK, image).map(Proof::into_lettering).expect("words");
            let commit = quickest(|| {
                let mut sheet = Sheet::new(base.clone()).expect("a sheet");
                timed(|| sheet.commit(label.clone()))
            });
            let undo = quickest(|| {
                let mut sheet = Sheet::new(base.clone()).expect("a sheet");
                sheet.commit(label.clone());
                sheet.commit(label.clone());
                timed(|| sheet.undo())
            });
            let ([wide, tall], lines) = (label.area().size(), text.lines().count());
            println!(
                "{name}, {em:.0} px letters on {lines} line(s), a {wide} x {tall} mask, atlas {:?}: \
                 first set {first:.1} ms, again {again:.1} ms, commit {commit:.1} ms, overlapping undo {undo:.1} ms",
                atlas(&typesetter),
            );
        }
    }
}
