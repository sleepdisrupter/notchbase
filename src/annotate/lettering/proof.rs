//! Words as set, with the layout kept for placing the caret and moving it between letters and lines.

use egui::text::{CCursor, Galley};
use egui::{Color32, ColorImage, Pos2, Rect, Vec2, vec2};

use super::{Lettering, Words};
use crate::annotate::rect::PixelRect;

pub struct Proof {
    lettering: Lettering,
    galley: Galley,
}

impl Proof {
    pub(super) fn pulled(atlas: &ColorImage, galley: Galley, words: Words, color: Color32, image: [usize; 2]) -> Self {
        let corner = words.origin.to_vec2();
        let area = PixelRect::covering(galley.mesh_bounds.translate(corner), image);
        let mut coverage = vec![0; area.width * area.height];
        for row in &galley.rows {
            for glyph in row.glyphs.iter().filter(|glyph| !glyph.uv_rect.is_nothing()) {
                let at = row.pos + corner + (glyph.pos + glyph.uv_rect.offset).round().to_vec2();
                let [left, top] = glyph.uv_rect.min.map(usize::from);
                let [right, bottom] = glyph.uv_rect.max.map(usize::from);
                let texels = PixelRect { x: left, y: top, width: right - left, height: bottom - top };
                press(&mut coverage, area, atlas, at, texels);
            }
        }
        let frame = galley.rect.translate(corner);
        Self { lettering: Lettering::new(color, area, coverage, frame, words), galley }
    }

    pub fn lettering(&self) -> &Lettering {
        &self.lettering
    }

    pub fn into_lettering(self) -> Lettering {
        self.lettering
    }

    pub fn caret(&self, index: usize) -> Rect {
        self.galley.pos_from_cursor(CCursor::new(index)).translate(self.corner())
    }

    pub fn index_at(&self, point: Pos2) -> usize {
        self.galley.cursor_from_pos(point.to_vec2() - self.corner()).index.0
    }

    pub fn index_above(&self, index: usize) -> usize {
        self.galley.cursor_up_one_row(&CCursor::new(index), None).0.index.0
    }

    pub fn index_below(&self, index: usize) -> usize {
        self.galley.cursor_down_one_row(&CCursor::new(index), None).0.index.0
    }

    pub fn line_start(&self, index: usize) -> usize {
        self.galley.cursor_begin_of_row(&CCursor::new(index)).index.0
    }

    pub fn line_end(&self, index: usize) -> usize {
        self.galley.cursor_end_of_row(&CCursor::new(index)).index.0
    }

    fn corner(&self) -> Vec2 {
        self.lettering.words().origin.to_vec2()
    }
}

fn press(coverage: &mut [u8], area: PixelRect, atlas: &ColorImage, at: Pos2, texels: PixelRect) {
    let placed = Rect::from_min_size(at, vec2(texels.width as f32, texels.height as f32));
    let inked = PixelRect::covering(placed, [area.right(), area.bottom()]).intersect(area);
    for y in inked.y..inked.bottom() {
        let [across, down] = [inked.x as f32 - at.x, y as f32 - at.y].map(|skipped| skipped as usize);
        let from = (texels.y + down) * atlas.width() + texels.x + across;
        let to = (y - area.y) * area.width + inked.x - area.x;
        let row = coverage[to..to + inked.width].iter_mut();
        for (level, texel) in row.zip(&atlas.pixels[from..from + inked.width]) {
            *level = (*level).max(texel.a());
        }
    }
}
