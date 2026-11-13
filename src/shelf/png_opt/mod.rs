//! A lossless PNG optimizer. It tries a palette and the plainest color layout with a few filters, keeps the smallest
//! file, and carries the color profile and metadata over.

mod decode;
mod layout;
mod pixels;
#[cfg(test)]
mod tests;
mod write;

use image::DynamicImage;
use png::{ColorType, Filter};

use super::icc;
use layout::Layout;
use pixels::Pixels;

const THOROUGH_SEARCH_MAX_PIXELS: u64 = 4_000_000;

#[derive(Debug, PartialEq)]
pub(super) enum Error {
    Unreadable,
    TooLarge,
    Animated,
    Unsupported,
    Encoding,
}

#[derive(Default)]
struct RenderingChunks {
    srgb: Option<png::SrgbRenderingIntent>,
    icc: Option<Vec<u8>>,
    gamma: Option<png::ScaledFloat>,
    chromaticities: Option<png::SourceChromaticities>,
    pixel_dims: Option<png::PixelDimensions>,
    exif: Option<Vec<u8>>,
}

impl RenderingChunks {
    fn keep_a_valid_color_profile(&mut self, has_color: bool) {
        self.icc = self.icc.take().filter(|profile| icc::matches(profile, has_color));
        if self.icc.is_some() {
            self.srgb = None;
        }
    }
}

pub(super) fn optimize(png_file: &[u8]) -> Result<Vec<u8>, Error> {
    let (pixels, chunks) = decode::decode(png_file)?;
    smallest_encoding(pixels, chunks)
}

pub(super) fn encode_image(image: &DynamicImage, icc: Option<Vec<u8>>) -> Result<Vec<u8>, Error> {
    smallest_encoding(Pixels::from_image(image), RenderingChunks { icc, ..RenderingChunks::default() })
}

fn smallest_encoding(mut pixels: Pixels, mut chunks: RenderingChunks) -> Result<Vec<u8>, Error> {
    if pixels.width == 0 || pixels.height == 0 {
        return Err(Error::Unsupported);
    }
    let layouts = lossless_layouts(&mut pixels, &mut chunks);
    let thorough = pixels.len() as u64 <= THOROUGH_SEARCH_MAX_PIXELS;
    let mut smallest: Option<Vec<u8>> = None;
    for layout in layouts {
        for &filter in filters_worth_trying(&layout, thorough) {
            let encoded = write::encode(pixels.width, pixels.height, &layout, &chunks, filter)?;
            if smallest.as_ref().is_none_or(|best| encoded.len() < best.len()) {
                smallest = Some(encoded);
            }
        }
    }
    smallest.ok_or(Error::Encoding)
}

fn lossless_layouts(pixels: &mut Pixels, chunks: &mut RenderingChunks) -> Vec<Layout> {
    chunks.keep_a_valid_color_profile(pixels.has_color());
    let profiled = chunks.icc.is_some();
    pixels.reduce_losslessly(!profiled);
    let palette_allowed = !profiled || pixels.has_color();
    let plain = Layout::plain(pixels);
    let mut layouts = Vec::with_capacity(2);
    if palette_allowed && let Some(palette) = Layout::palette(pixels) {
        let plain_gray_is_as_small = plain.color == ColorType::Grayscale && plain.depth <= palette.depth;
        if !plain_gray_is_as_small {
            layouts.push(palette);
        }
    }
    layouts.push(plain);
    layouts
}

fn filters_worth_trying(layout: &Layout, thorough: bool) -> &'static [Filter] {
    let packed = layout.color == ColorType::Indexed || layout.depth < 8;
    match (packed, thorough) {
        (true, true) => &[Filter::NoFilter, Filter::Adaptive],
        (true, false) => &[Filter::NoFilter],
        (false, true) => &[Filter::Adaptive, Filter::MinEntropy],
        (false, false) => &[Filter::Adaptive],
    }
}
