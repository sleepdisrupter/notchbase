//! A captured image in RGBA, and the small thumbnail of it that the notch shows.

use std::ops::Range;

use egui::Color32;

#[derive(Clone)]
pub struct Screenshot {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

impl Screenshot {
    /// A copy at most `max` pixels on its longer side. It never comes out larger than the original.
    pub fn thumbnail(&self, max: usize) -> egui::ColorImage {
        let scale = (self.width.max(self.height) as f32 / max.max(1) as f32).max(1.0);
        let [columns, rows] = [self.width, self.height].map(|side| ((side as f32 / scale).round() as usize).max(1));
        let pixels = (0..rows)
            .flat_map(|row| (0..columns).map(move |column| (column, row)))
            .map(|(column, row)| self.average(span(column, columns, self.width), span(row, rows, self.height)))
            .collect();
        egui::ColorImage::new([columns, rows], pixels)
    }

    /// The mean color of a block, from about four samples along each side.
    fn average(&self, xs: Range<usize>, ys: Range<usize>) -> Color32 {
        let stride = |range: &Range<usize>| (range.len() / 4).max(1);
        let (step_x, step_y) = (stride(&xs), stride(&ys));
        let mut sum = [0u32; 3];
        let mut samples = 0u32;
        for y in ys.step_by(step_y) {
            for x in xs.clone().step_by(step_x) {
                let i = (y * self.width + x) * 4;
                for (total, &channel) in sum.iter_mut().zip(&self.rgba[i..i + 3]) {
                    *total += u32::from(channel);
                }
                samples += 1;
            }
        }
        let [r, g, b] = sum.map(|total| (total / samples.max(1)) as u8);
        Color32::from_rgb(r, g, b)
    }
}

fn span(index: usize, cells: usize, len: usize) -> Range<usize> {
    let start = index * len / cells;
    start..((index + 1) * len / cells).max(start + 1).min(len)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(width: usize, height: usize, rgb: [u8; 3]) -> Screenshot {
        let rgba = (0..width * height).flat_map(|_| [rgb[0], rgb[1], rgb[2], 255]).collect();
        Screenshot { width, height, rgba }
    }

    #[test]
    fn thumbnails_keep_aspect_no_upscale() {
        let wide = solid(1920, 1080, [10, 20, 30]).thumbnail(96);
        assert_eq!(wide.size, [96, 54]);
        assert!(wide.pixels.iter().all(|&p| p == Color32::from_rgb(10, 20, 30)));
        assert_eq!(solid(40, 30, [0, 0, 0]).thumbnail(96).size, [40, 30]);
        assert_eq!(solid(1, 500, [0, 0, 0]).thumbnail(96).size, [1, 96]);
        assert_eq!(solid(0, 0, [0, 0, 0]).thumbnail(96).size, [1, 1]);
    }

    #[test]
    fn thumbnail_pixel_is_block_mean() {
        let rows = [[0u8, 0, 0, 255]; 4].concat().into_iter().chain([[100u8, 60, 20, 255]; 4].concat());
        let shot = Screenshot { width: 4, height: 2, rgba: rows.collect() };
        let thumbnail = shot.thumbnail(2);
        assert_eq!(thumbnail.size, [2, 1]);
        assert_eq!(thumbnail.pixels, vec![Color32::from_rgb(50, 30, 10); 2]);
    }

    #[test]
    fn spans_tile_without_gaps_or_overlap() {
        for (cells, len) in [(96, 1920), (54, 1080), (7, 10), (3, 3), (96, 97)] {
            let spans: Vec<_> = (0..cells).map(|i| span(i, cells, len)).collect();
            assert_eq!(spans[0].start, 0);
            assert_eq!(spans[cells - 1].end, len);
            assert!(spans.windows(2).all(|pair| pair[0].end == pair[1].start && !pair[0].is_empty()));
        }
    }
}
