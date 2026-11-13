//! Sets words into a coverage mask using its own font atlas, one letter size at a time. A full atlas is started over,
//! and a panic in the fonts refuses the words instead.

#[cfg(test)]
mod tests;
#[cfg(test)]
mod timing;

use std::collections::BTreeMap;
use std::panic::{self, AssertUnwindSafe};
use std::sync::Arc;

use egui::epaint::FontColorTransferFunction;
use egui::epaint::text::{TextOptions, layout};
use egui::text::{Fonts, LayoutJob, TextFormat};
use egui::{Color32, FontDefinitions, FontFamily, FontId};

use super::Words;
use super::proof::Proof;

const ATLAS_SIDE: usize = 4096;
const WIDEST_LETTER_OF_ATLAS: f32 = 0.5;
const TALLEST_ATLAS_AT_REST: usize = 512;
const WHOLE_PIXELS: f32 = 1.0;
const SHORTEST: f32 = 6.0;
const TALLEST: f32 = 320.0;
const LEADING: f32 = 1.25;
const OVERFLOWED: f32 = 1.0;

pub struct Typesetter {
    definitions: FontDefinitions,
    family: FontFamily,
    atlas_side: usize,
    fonts: Option<Fonts>,
    em_in_the_atlas: Option<f32>,
}

struct Overflowed;

impl Typesetter {
    pub fn new(definitions: &FontDefinitions, family: &FontFamily) -> Self {
        Self::with_atlas(definitions, family, ATLAS_SIDE)
    }

    pub fn set(&mut self, words: &Words, color: Color32, image: [usize; 2]) -> Option<Proof> {
        self.guarded(|typesetter| {
            let first = typesetter.attempt(words, color, image);
            first.or_else(|Overflowed| typesetter.attempt(words, color, image)).ok().flatten()
        })
    }

    /// Lets a tall atlas go once typing ends, to give its memory back.
    pub fn rest(&mut self) {
        if self.fonts.as_ref().is_some_and(|fonts| fonts.font_image_size()[1] > TALLEST_ATLAS_AT_REST) {
            self.fonts = None;
        }
    }

    fn with_atlas(definitions: &FontDefinitions, family: &FontFamily, atlas_side: usize) -> Self {
        let (definitions, family) = only(definitions, family);
        let mut typesetter = Self { definitions, family, atlas_side, fonts: None, em_in_the_atlas: None };
        typesetter.guarded(|typesetter| {
            typesetter.fonts();
        });
        typesetter
    }

    fn guarded<Made: Default>(&mut self, work: impl FnOnce(&mut Self) -> Made) -> Made {
        let made = panic::catch_unwind(AssertUnwindSafe(|| work(self)));
        made.unwrap_or_else(|_| {
            self.fonts = None;
            Made::default()
        })
    }

    fn attempt(&mut self, words: &Words, color: Color32, image: [usize; 2]) -> Result<Option<Proof>, Overflowed> {
        let Some(words) = settable(words) else { return Ok(None) };
        let font = FontId::new(words.em, self.family.clone());
        let widest = self.atlas_side as f32 * WIDEST_LETTER_OF_ATLAS;
        let fonts = self.fonts_for(words.em);
        if !fits(fonts, &font, &words.text, widest) {
            return Ok(None);
        }
        let galley = layout(&mut fonts.fonts, WHOLE_PIXELS, Arc::new(job(&words.text, font)));
        if fonts.font_atlas_fill_ratio() >= OVERFLOWED {
            self.fonts = None;
            return Err(Overflowed);
        }
        Ok(Some(Proof::pulled(fonts.texture_atlas().image(), galley, words, color, image)))
    }

    fn fonts_for(&mut self, em: f32) -> &mut Fonts {
        if self.em_in_the_atlas.is_some_and(|held| held != em) {
            self.fonts = None;
        }
        self.em_in_the_atlas = Some(em);
        self.fonts()
    }

    fn fonts(&mut self) -> &mut Fonts {
        let (definitions, options) = (&self.definitions, options(self.atlas_side));
        self.fonts.get_or_insert_with(|| Fonts::new(options, definitions.clone()))
    }
}

fn only(definitions: &FontDefinitions, family: &FontFamily) -> (FontDefinitions, FontFamily) {
    let family = if definitions.families.contains_key(family) { family.clone() } else { FontFamily::Proportional };
    let faces = definitions.families.get(&family).cloned().unwrap_or_default();
    let named = definitions.font_data.iter().filter(|(name, _)| faces.contains(name));
    let font_data = named.map(|(name, data)| (name.clone(), data.clone())).collect();
    (FontDefinitions { font_data, families: BTreeMap::from([(family.clone(), faces)]) }, family)
}

fn options(atlas_side: usize) -> TextOptions {
    TextOptions {
        max_texture_side: atlas_side,
        color_transfer_function: FontColorTransferFunction::Off,
        font_hinting: false,
        subpixel_binning: false,
    }
}

fn settable(words: &Words) -> Option<Words> {
    let em = if words.em.is_nan() { SHORTEST } else { words.em.clamp(SHORTEST, TALLEST) };
    words.origin.is_finite().then(|| Words { text: words.text.clone(), origin: words.origin.round(), em })
}

fn fits(fonts: &mut Fonts, font: &FontId, text: &str, widest: f32) -> bool {
    let mut measure = fonts.with_pixels_per_point(WHOLE_PIXELS);
    text.chars().all(|letter| measure.glyph_width(font, letter) <= widest)
}

fn job(text: &str, font: FontId) -> LayoutJob {
    let line_height = Some((font.size * LEADING).round());
    LayoutJob::single_section(text.to_owned(), TextFormat { line_height, ..TextFormat::simple(font, Color32::WHITE) })
}
