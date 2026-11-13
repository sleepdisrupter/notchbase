//! Inks marks into the pixels. A line is traced as round capsules into a coverage mask, then mixed in its colour.

mod capsule;
mod row;
mod stamp;
#[cfg(test)]
mod tests;

use std::ops::Range;

use egui::{Color32, Pos2};

use self::capsule::Capsule;
use super::mark::Mark;
use super::rect::PixelRect;
use super::stroke::Stroke;

const CHANNELS: usize = 4;
const FULL: u8 = u8::MAX;
const PIXEL_CENTRE: f32 = 0.5;

pub(super) fn apply(pixels: &mut [u8], image: [usize; 2], clip: PixelRect, mark: &Mark) {
    match mark {
        Mark::Line(stroke) => ink(pixels, image, clip, stroke),
        Mark::Text(lettering) => stamp::stamp(pixels, image, clip, lettering),
    }
}

pub(super) fn ink(pixels: &mut [u8], image: [usize; 2], clip: PixelRect, stroke: &Stroke) {
    let whole = PixelRect::of_image(image);
    let area = PixelRect::covering(stroke.bounds(), image).intersect(clip);
    if area.is_empty() || pixels.len() != whole.width * whole.height * CHANNELS {
        return;
    }
    let mut mask = Mask::new(area);
    let visible = area.to_rect();
    for (_, points) in stroke.runs().filter(|(reach, _)| reach.intersects(visible)) {
        mask.trace(points, stroke.radius());
    }
    mask.tint(pixels, whole.width, stroke.color());
}

struct Mask {
    area: PixelRect,
    coverage: Vec<u8>,
    touched: Vec<Range<usize>>,
}

impl Mask {
    fn new(area: PixelRect) -> Self {
        Self { area, coverage: vec![0; area.width * area.height], touched: vec![0..0; area.height] }
    }

    fn trace(&mut self, points: &[Pos2], radius: f32) {
        if let Some(&tap) = points.first() {
            self.cover(&Capsule::new(tap, tap, radius));
        }
        for pair in points.windows(2) {
            self.cover(&Capsule::new(pair[0], pair[1], radius));
        }
    }

    fn cover(&mut self, capsule: &Capsule) {
        let across = self.area.x..self.area.right();
        if capsule.is_beside(&across) {
            return;
        }
        for y in capsule.rows(self.area.y..self.area.bottom()) {
            let row = (y - self.area.y) * self.area.width;
            let line = &mut self.coverage[row..row + self.area.width];
            let covered = row::cover(line, &across, capsule, y as f32 + PIXEL_CENTRE);
            if !covered.is_empty() {
                let touched = &mut self.touched[y - self.area.y];
                *touched = widened(touched, covered);
            }
        }
    }

    fn tint(&self, pixels: &mut [u8], image_width: usize, color: Color32) {
        let ink = [color.r(), color.g(), color.b()];
        for (row, touched) in self.touched.iter().enumerate() {
            let levels = row * self.area.width;
            let line = (self.area.y + row) * image_width;
            for x in touched.clone() {
                let level = self.coverage[levels + x - self.area.x];
                if level == 0 {
                    continue;
                }
                let at = (line + x) * CHANNELS;
                for (channel, ink) in pixels[at..].iter_mut().zip(ink) {
                    *channel = lerp(*channel, ink, level);
                }
            }
        }
    }
}

fn widened(touched: &Range<usize>, columns: Range<usize>) -> Range<usize> {
    if touched.is_empty() { columns } else { touched.start.min(columns.start)..touched.end.max(columns.end) }
}

fn lerp(under: u8, ink: u8, level: u8) -> u8 {
    let (under, ink, level, full) = (u32::from(under), u32::from(ink), u32::from(level), u32::from(FULL));
    ((under * (full - level) + ink * level + full / 2) / full) as u8
}
