use egui::{Color32, ColorImage, Pos2};

use super::lettering::{Lettering, Words};
use super::rect::PixelRect;
use super::stroke::Stroke;
use crate::capture::Screenshot;

const MULTIPLIER: u64 = 6_364_136_223_846_793_005;
const INCREMENT: u64 = 1_442_695_040_888_963_407;
const WRITTEN: &str = "words";
const SOLID: u8 = u8::MAX;

pub(super) fn solid(width: usize, height: usize, [r, g, b]: [u8; 3]) -> Screenshot {
    Screenshot { width, height, rgba: [r, g, b, u8::MAX].repeat(width * height) }
}

pub(super) fn textured(width: usize, height: usize) -> Screenshot {
    let texel = |index: usize| {
        let (x, y) = (index % width, index / width);
        [(x * 7 + y * 3) as u8, (x * 2 + y * 11) as u8, (x ^ y) as u8, u8::MAX]
    };
    Screenshot { width, height, rgba: (0..width * height).flat_map(texel).collect() }
}

pub(super) fn through(color: Color32, width: f32, samples: &[[f32; 2]]) -> Stroke {
    let samples: Vec<Pos2> = samples.iter().map(|&[x, y]| Pos2::new(x, y)).collect();
    Stroke::from_samples(color, width, &samples).expect("a stroke")
}

pub(super) fn line(color: Color32, width: f32, from: [f32; 2], to: [f32; 2]) -> Stroke {
    through(color, width, &[from, to])
}

pub(super) fn lettering(color: Color32, area: PixelRect, levels: Vec<u8>) -> Lettering {
    let words = Words { text: WRITTEN.to_owned(), origin: area.to_rect().min, em: area.height as f32 };
    Lettering::new(color, area, levels, area.to_rect(), words)
}

pub(super) fn block(color: Color32, area: PixelRect) -> Lettering {
    lettering(color, area, vec![SOLID; area.width * area.height])
}

pub(super) fn as_image(shot: &Screenshot) -> ColorImage {
    ColorImage::from_rgba_unmultiplied([shot.width, shot.height], &shot.rgba)
}

pub(super) struct Dice(pub u64);

impl Dice {
    pub(super) fn below(&mut self, sides: u32) -> u32 {
        self.0 = self.0.wrapping_mul(MULTIPLIER).wrapping_add(INCREMENT);
        ((self.0 >> 33) as u32) % sides
    }

    pub(super) fn spot(&mut self, [width, height]: [usize; 2]) -> [f32; 2] {
        [width, height].map(|side| self.below(side as u32 * 5 / 4) as f32 - side as f32 / 8.0)
    }
}
