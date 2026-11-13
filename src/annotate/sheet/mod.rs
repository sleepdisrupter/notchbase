//! The sheet: the screenshot, the marks on it and the pixels with them inked in. Changes redraw only the area they
//! touch, and a block being edited is lifted out of the picture until it's settled.

mod history;
#[cfg(test)]
mod tests;
#[cfg(test)]
pub(super) mod timing;

use std::ops::Range;
use std::sync::Arc;

use egui::{Color32, ColorImage, Pos2, Rect};

use self::history::Step;
use super::lettering::Lettering;
use super::mark::Mark;
use super::raster;
use super::rect::PixelRect;
use crate::capture::Screenshot;

const CHANNELS: usize = 4;

pub struct Sheet {
    base: Arc<Screenshot>,
    pixels: Vec<u8>,
    marks: Vec<Mark>,
    done: Vec<Step>,
    undone: Vec<Step>,
    lifted: Option<usize>,
    revision: u64,
}

impl Sheet {
    pub fn new(base: Arc<Screenshot>) -> Option<Self> {
        let drawable = base.width > 0 && base.height > 0 && base.rgba.len() == base.width * base.height * CHANNELS;
        drawable.then(|| Self {
            pixels: base.rgba.clone(),
            base,
            marks: Vec::new(),
            done: Vec::new(),
            undone: Vec::new(),
            lifted: None,
            revision: 0,
        })
    }

    pub fn base(&self) -> &Arc<Screenshot> {
        &self.base
    }

    pub fn size(&self) -> [usize; 2] {
        [self.base.width, self.base.height]
    }

    pub fn marks(&self) -> &[Mark] {
        &self.marks
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn can_undo(&self) -> bool {
        !self.done.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.undone.is_empty()
    }

    pub fn words(&self, at: usize) -> Option<&Lettering> {
        self.marks.get(at)?.text()
    }

    pub fn words_at(&self, point: Pos2) -> Option<usize> {
        self.marks.iter().rposition(|mark| mark.text().is_some_and(|lettering| lettering.is_at(point)))
    }

    pub fn lift(&mut self, at: usize) -> Option<PixelRect> {
        let hidden = self.marks.get(at)?.bounds();
        let settled = self.settle();
        self.lifted = Some(at);
        Some(settled.union(self.repaint(hidden)))
    }

    pub fn settle(&mut self) -> PixelRect {
        let Some(at) = self.lifted.take() else { return PixelRect::EMPTY };
        self.repaint(self.bounds_at(at))
    }

    pub fn image(&self) -> ColorImage {
        self.read(PixelRect::of_image(self.size()))
    }

    pub fn patch(&self, rect: PixelRect) -> Option<ColorImage> {
        let inside = rect.intersect(PixelRect::of_image(self.size()));
        (!inside.is_empty()).then(|| self.read(inside))
    }

    fn read(&self, rect: PixelRect) -> ColorImage {
        let pixels = (rect.y..rect.bottom())
            .flat_map(|y| self.pixels[self.row(y, rect)].as_chunks::<CHANNELS>().0)
            .map(|&[r, g, b, _]| Color32::from_rgb(r, g, b))
            .collect();
        ColorImage::new(rect.size(), pixels)
    }

    fn row(&self, y: usize, rect: PixelRect) -> Range<usize> {
        let start = (y * self.base.width + rect.x) * CHANNELS;
        start..start + rect.width * CHANNELS
    }

    fn ink_latest(&mut self) -> PixelRect {
        let Some(mark) = self.marks.last() else { return PixelRect::EMPTY };
        let size = self.size();
        let clip = PixelRect::covering(mark.bounds(), size);
        raster::apply(&mut self.pixels, size, clip, mark);
        clip
    }

    fn repaint(&mut self, changed: Rect) -> PixelRect {
        let size = self.size();
        let clip = PixelRect::covering(changed, size);
        for y in clip.y..clip.bottom() {
            let row = self.row(y, clip);
            self.pixels[row.clone()].copy_from_slice(&self.base.rgba[row]);
        }
        let lifted = self.lifted;
        for (_, mark) in self.marks.iter().enumerate().filter(|(at, _)| lifted != Some(*at)) {
            raster::apply(&mut self.pixels, size, clip, mark);
        }
        clip
    }

    fn bounds_at(&self, at: usize) -> Rect {
        self.marks.get(at).map_or(Rect::NOTHING, Mark::bounds)
    }
}
