mod caret;
mod guards;
mod setting;

use egui::{Color32, FontDefinitions, FontFamily, pos2};

use crate::annotate::lettering::{Lettering, Proof, Typesetter, Words};
use crate::ui::color;

const IMAGE: [usize; 2] = [900, 500];
const INK: Color32 = color::RED;
const PLACE: [f32; 2] = [40.0, 30.0];
const SMALLEST_ATLAS: usize = 1024;

fn typesetter() -> Typesetter {
    Typesetter::new(&FontDefinitions::default(), &FontFamily::Proportional)
}

fn cramped() -> Typesetter {
    Typesetter::with_atlas(&FontDefinitions::default(), &FontFamily::Proportional, SMALLEST_ATLAS)
}

fn words(text: &str, [x, y]: [f32; 2], em: f32) -> Words {
    Words { text: text.to_owned(), origin: pos2(x, y), em }
}

fn proof(text: &str, at: [f32; 2], em: f32) -> Proof {
    typesetter().set(&words(text, at, em), INK, IMAGE).expect("words that can be set")
}

fn set(text: &str, at: [f32; 2], em: f32) -> Lettering {
    proof(text, at, em).into_lettering()
}
