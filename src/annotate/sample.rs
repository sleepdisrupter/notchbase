//! A made-up screenshot, a page of lines on a sky, for previewing the annotate window.

use crate::capture::Screenshot;

const PAPER: [u8; 3] = [246, 245, 242];
const TEXT: [u8; 3] = [196, 198, 204];
const SKY: [[f32; 3]; 2] = [[64.0, 92.0, 150.0], [214.0, 150.0, 128.0]];
const PAGE_ACROSS: std::ops::Range<f32> = 0.12..0.88;
const PAGE_DOWN: std::ops::Range<f32> = 0.14..0.9;
const FIRST_LINE: f32 = 0.2;
const LINES_PER_PAGE: f32 = 14.0;
const LINE_THICKNESS: f32 = 0.28;
const LINE_STARTS: f32 = 0.18;
const SHORTEST_LINE_ENDS: f32 = 0.42;
const LINE_GROWTH: f32 = 0.04;

pub fn sample([width, height]: [usize; 2]) -> Screenshot {
    let rgba = (0..width * height).flat_map(|index| {
        let across = (index % width) as f32 / width as f32;
        let down = (index / width) as f32 / height as f32;
        let [r, g, b] = pixel(across, down);
        [r, g, b, u8::MAX]
    });
    Screenshot { width, height, rgba: rgba.collect() }
}

fn pixel(across: f32, down: f32) -> [u8; 3] {
    if !(PAGE_ACROSS.contains(&across) && PAGE_DOWN.contains(&down)) {
        return std::array::from_fn(|channel| (SKY[0][channel] + (SKY[1][channel] - SKY[0][channel]) * down) as u8);
    }
    let line = (down - FIRST_LINE) * LINES_PER_PAGE;
    let reaches = SHORTEST_LINE_ENDS + LINE_GROWTH * line.floor();
    let written = down > FIRST_LINE && line.fract() < LINE_THICKNESS && across > LINE_STARTS && across < reaches;
    if written { TEXT } else { PAPER }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opaque_page_of_lines_on_a_sky() {
        let shot = sample([200, 100]);
        assert_eq!((shot.width, shot.height, shot.rgba.len()), (200, 100, 200 * 100 * 4));
        assert!(shot.rgba.as_chunks::<4>().0.iter().all(|pixel| pixel[3] == u8::MAX));
        let at = |x: usize, y: usize| <[u8; 3]>::try_from(&shot.rgba[(y * 200 + x) * 4..][..3]).expect("a pixel");
        assert_eq!(at(100, 16), PAPER, "the margin above the first line is blank paper");
        assert_eq!(at(60, 21), TEXT, "the first line is written");
        assert_ne!(at(4, 4), at(4, 96), "the sky fades from top to bottom");
    }

    #[test]
    fn no_pixels_is_empty_not_a_panic() {
        assert!(sample([0, 0]).rgba.is_empty());
    }
}
