//! The typesetter for one window, set up with the app's fonts and the screenshot's size.

use egui::Color32;

use crate::annotate::{Proof, Typesetter, Words};
use crate::ui::font;

pub(in crate::annotate::editor) struct Setter {
    typesetter: Typesetter,
    image: [usize; 2],
}

impl Setter {
    pub(in crate::annotate::editor) fn new(ctx: &egui::Context, image: [usize; 2]) -> Self {
        let definitions = ctx.fonts(|fonts| fonts.definitions().clone());
        Self { typesetter: Typesetter::new(&definitions, &font::headline().family), image }
    }

    pub(in crate::annotate::editor) fn rest(&mut self) {
        self.typesetter.rest();
    }

    pub(super) fn set(&mut self, words: &Words, color: Color32) -> Option<Proof> {
        self.typesetter.set(words, color, self.image)
    }

    pub(super) fn image(&self) -> [usize; 2] {
        self.image
    }
}
