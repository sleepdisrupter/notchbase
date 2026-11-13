//! Decodes artwork, center-crops it to a square and shrinks it to 96 by 96 for the notch.

use egui::ColorImage;
use image::imageops::{self, FilterType};

const SIDE: u32 = 96;

pub fn decode(bytes: &[u8]) -> Option<ColorImage> {
    let image = image::load_from_memory(bytes).ok()?.into_rgba8();
    let (w, h) = image.dimensions();
    if w == 0 || h == 0 {
        return None;
    }
    let side = w.min(h);
    let square = imageops::crop_imm(&image, (w - side) / 2, (h - side) / 2, side, side).to_image();
    let small = imageops::resize(&square, SIDE, SIDE, FilterType::Triangle);
    Some(ColorImage::from_rgba_unmultiplied([SIDE as usize; 2], small.as_raw()))
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use image::{ImageFormat, Rgba, RgbaImage};

    use super::*;

    #[test]
    fn artwork_is_cropped_to_a_square() {
        let mut wide = RgbaImage::from_pixel(320, 180, Rgba([255, 0, 0, 255]));
        for y in 0..180 {
            for x in 70..250 {
                wide.put_pixel(x, y, Rgba([0, 0, 255, 255]));
            }
        }
        let mut png = Vec::new();
        wide.write_to(&mut Cursor::new(&mut png), ImageFormat::Png).unwrap();
        let art = decode(&png).expect("decodes");
        assert_eq!(art.size, [SIDE as usize; 2]);
        assert!(art.pixels.iter().all(|p| p.b() > 200 && p.r() < 40), "only the middle square survives");
        assert!(decode(b"not an image").is_none());
    }
}
