//! Inks a block of words from its coverage mask, with the same mixing as a line.

use super::{CHANNELS, lerp};
use crate::annotate::lettering::Lettering;
use crate::annotate::rect::PixelRect;

pub(super) fn stamp(pixels: &mut [u8], image: [usize; 2], clip: PixelRect, lettering: &Lettering) {
    let whole = PixelRect::of_image(image);
    let (area, mask) = (lettering.area(), lettering.coverage());
    let inside = area.intersect(clip).intersect(whole);
    let fits_the_image = pixels.len() == whole.width * whole.height * CHANNELS;
    let fits_the_area = area.width.checked_mul(area.height) == Some(mask.len());
    if inside.is_empty() || !fits_the_image || !fits_the_area {
        return;
    }
    let color = lettering.color();
    let ink = [color.r(), color.g(), color.b()];
    for y in inside.y..inside.bottom() {
        let first = (y - area.y) * area.width + inside.x - area.x;
        let line = (y * whole.width + inside.x) * CHANNELS;
        let row = pixels[line..line + inside.width * CHANNELS].as_chunks_mut::<CHANNELS>().0;
        let levels = &mask[first..first + inside.width];
        for (pixel, level) in row.iter_mut().zip(levels).filter(|(_, level)| **level > 0) {
            tint(pixel, ink, *level);
        }
    }
}

fn tint(pixel: &mut [u8; CHANNELS], ink: [u8; 3], level: u8) {
    for (channel, ink) in pixel.iter_mut().zip(ink) {
        *channel = lerp(*channel, ink, level);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotate::raster::ink;
    use crate::annotate::testing::{block, lettering, line, solid, textured};
    use crate::capture::Screenshot;
    use crate::ui::color;

    const WHITE: [u8; 3] = [255, 255, 255];
    const IMAGE: [usize; 2] = [40, 20];
    const AREA: PixelRect = PixelRect { x: 10, y: 6, width: 4, height: 2 };
    const LEVELS: [u8; 8] = [0, 1, 128, 255, 255, 134, 77, 0];

    fn stamped(base: &Screenshot, lettering: &Lettering) -> Screenshot {
        let mut shot = base.clone();
        let size = [shot.width, shot.height];
        stamp(&mut shot.rgba, size, PixelRect::of_image(size), lettering);
        shot
    }

    fn pixel(shot: &Screenshot, x: usize, y: usize) -> [u8; CHANNELS] {
        let at = (y * shot.width + x) * CHANNELS;
        [shot.rgba[at], shot.rgba[at + 1], shot.rgba[at + 2], shot.rgba[at + 3]]
    }

    #[test]
    fn words_mix_like_a_line() {
        let base = textured(IMAGE[0], IMAGE[1]);
        let written = stamped(&base, &lettering(color::RED, AREA, LEVELS.to_vec()));
        for (index, level) in LEVELS.into_iter().enumerate() {
            let (x, y) = (AREA.x + index % AREA.width, AREA.y + index / AREA.width);
            let [under, ink] = [pixel(&base, x, y), color::RED.to_array()];
            let mixed: [u8; 3] = std::array::from_fn(|channel| lerp(under[channel], ink[channel], level));
            assert_eq!(pixel(&written, x, y)[..3], mixed, "{x},{y} at level {level}");
        }
        let soft_line = line(color::RED, 4.0, [10.0, 10.5], [30.0, 10.5]);
        let mut drawn = solid(IMAGE[0], IMAGE[1], WHITE);
        ink(&mut drawn.rgba, IMAGE, PixelRect::of_image(IMAGE), &soft_line);
        let same_level = lettering(color::RED, PixelRect { x: 20, y: 8, width: 1, height: 1 }, vec![128]);
        let written = stamped(&solid(IMAGE[0], IMAGE[1], WHITE), &same_level);
        assert_eq!(pixel(&written, 20, 8), pixel(&drawn, 20, 8));
        assert_eq!(pixel(&written, 20, 8), [245, 169, 172, 255]);
    }

    #[test]
    fn full_is_exact_ink_none_is_untouched() {
        let base = textured(IMAGE[0], IMAGE[1]);
        let written = stamped(&base, &lettering(color::GREEN, AREA, LEVELS.to_vec()));
        let [r, g, b, _] = color::GREEN.to_array();
        assert_eq!((pixel(&written, 13, 6), pixel(&written, 10, 7)), ([r, g, b, 255], [r, g, b, 255]));
        assert_eq!((pixel(&written, 10, 6), pixel(&written, 13, 7)), (pixel(&base, 10, 6), pixel(&base, 13, 7)));
        for (index, (after, before)) in written.rgba.iter().zip(&base.rgba).enumerate() {
            let (x, y) = (index / CHANNELS % IMAGE[0], index / CHANNELS / IMAGE[0]);
            let inside = (AREA.x..AREA.right()).contains(&x) && (AREA.y..AREA.bottom()).contains(&y);
            assert!(inside || after == before, "{x},{y}");
        }
    }

    #[test]
    fn clip_limits_lettering() {
        let base = textured(96, 80);
        let area = PixelRect { x: 20, y: 5, width: 50, height: 60 };
        let levels: Vec<u8> = (0..area.width * area.height).map(|index| (index * 37 % 256) as u8).collect();
        let words = lettering(color::YELLOW, area, levels);
        let whole = stamped(&base, &words);
        let clip = PixelRect { x: 30, y: 10, width: 25, height: 44 };
        let mut clipped = base.clone();
        stamp(&mut clipped.rgba, [96, 80], clip, &words);
        for (index, pixel) in clipped.rgba.as_chunks::<CHANNELS>().0.iter().enumerate() {
            let (x, y) = (index % 96, index / 96);
            let inside = (clip.x..clip.right()).contains(&x) && (clip.y..clip.bottom()).contains(&y);
            let expected = if inside { &whole } else { &base };
            assert_eq!(pixel[..], expected.rgba[index * CHANNELS..(index + 1) * CHANNELS], "{x},{y}");
        }
        assert!(clipped.rgba != base.rgba && clipped.rgba != whole.rgba);
    }

    #[test]
    fn lettering_leaves_alpha_alone() {
        let mut glassy = textured(IMAGE[0], IMAGE[1]);
        glassy
            .rgba
            .as_chunks_mut::<CHANNELS>()
            .0
            .iter_mut()
            .enumerate()
            .for_each(|(index, pixel)| pixel[3] = index as u8);
        let written = stamped(&glassy, &block(color::INK, PixelRect { x: 4, y: 2, width: 30, height: 16 }));
        assert!(written.rgba != glassy.rgba);
        let alpha = |shot: &Screenshot| shot.rgba.iter().skip(3).step_by(CHANNELS).copied().collect::<Vec<u8>>();
        assert_eq!(alpha(&written), alpha(&glassy));
    }

    #[test]
    fn only_the_part_on_the_image_inks() {
        let base = solid(IMAGE[0], IMAGE[1], WHITE);
        let hanging = block(color::INK, PixelRect { x: 36, y: 16, width: 10, height: 10 });
        let written = stamped(&base, &hanging);
        let [r, g, b, _] = color::INK.to_array();
        assert_eq!((pixel(&written, 36, 16), pixel(&written, 39, 19)), ([r, g, b, 255], [r, g, b, 255]));
        assert_eq!((pixel(&written, 35, 16), pixel(&written, 36, 15)), ([255; 4], [255; 4]));
        let outside = block(color::INK, PixelRect { x: 60, y: 30, width: 4, height: 4 });
        assert!(stamped(&base, &outside).rgba == base.rgba);
    }

    #[test]
    fn wrong_sized_mask_is_left_alone() {
        let base = textured(IMAGE[0], IMAGE[1]);
        let short_mask = lettering(color::RED, AREA, vec![255; 5]);
        assert!(stamped(&base, &short_mask).rgba == base.rgba);
        let endless = PixelRect { x: 0, y: 0, width: usize::MAX, height: 2 };
        assert!(stamped(&base, &lettering(color::RED, endless, vec![255; 8])).rgba == base.rgba);
        let mut short = vec![9u8; 100];
        stamp(&mut short, IMAGE, PixelRect::of_image(IMAGE), &block(color::RED, AREA));
        assert_eq!(short, vec![9u8; 100]);
        let mut empty = Vec::new();
        stamp(&mut empty, [0, 0], PixelRect::EMPTY, &block(color::RED, AREA));
        assert!(empty.is_empty());
        assert!(stamped(&base, &Lettering::empty()).rgba == base.rgba);
    }
}
