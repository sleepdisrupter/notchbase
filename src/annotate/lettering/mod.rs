//! Words on the picture. `Words` is what was typed and where, and a `Lettering` adds its colour and the mask it inks.

mod proof;
#[cfg(test)]
mod tests;
mod typeset;

use std::sync::Arc;

use egui::{Color32, Pos2, Rect};

pub use self::proof::Proof;
pub use self::typeset::Typesetter;
use super::rect::PixelRect;

#[derive(Clone, Debug, PartialEq)]
pub struct Words {
    pub text: String,
    pub origin: Pos2,
    pub em: f32,
}

impl Words {
    pub fn is_blank(&self) -> bool {
        self.text.trim().is_empty()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Lettering {
    color: Color32,
    area: PixelRect,
    coverage: Arc<[u8]>,
    frame: Rect,
    words: Words,
}

impl Lettering {
    pub fn new(color: Color32, area: PixelRect, coverage: Vec<u8>, frame: Rect, words: Words) -> Self {
        let color = Color32::from_rgb(color.r(), color.g(), color.b());
        Self { color, area, coverage: coverage.into(), frame, words }
    }

    pub fn empty() -> Self {
        let words = Words { text: String::new(), origin: Pos2::ZERO, em: 0.0 };
        Self::new(Color32::BLACK, PixelRect::EMPTY, Vec::new(), Rect::NOTHING, words)
    }

    pub fn color(&self) -> Color32 {
        self.color
    }

    pub fn area(&self) -> PixelRect {
        self.area
    }

    pub fn coverage(&self) -> &[u8] {
        &self.coverage
    }

    pub fn frame(&self) -> Rect {
        self.frame
    }

    pub fn words(&self) -> &Words {
        &self.words
    }

    pub fn bounds(&self) -> Rect {
        if self.area.is_empty() { Rect::NOTHING } else { self.area.to_rect() }
    }

    pub fn is_at(&self, point: Pos2) -> bool {
        !self.words.is_blank() && self.frame.contains(point)
    }
}
