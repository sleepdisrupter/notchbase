//! Writes one candidate PNG with the chosen layout and filter at the best deflate level.

use std::borrow::Cow;

use png::{BitDepth, DeflateCompression, Filter, Info};

use super::layout::Layout;
use super::{Error, RenderingChunks};

const BEST_DEFLATE_LEVEL: u8 = 9;

pub(super) fn encode(
    width: u32,
    height: u32,
    layout: &Layout,
    chunks: &RenderingChunks,
    filter: Filter,
) -> Result<Vec<u8>, Error> {
    let mut info = Info::with_size(width, height);
    info.color_type = layout.color;
    info.bit_depth = bit_depth(layout.depth);
    info.palette = layout.palette.as_deref().map(Cow::Borrowed);
    info.trns = layout.transparency.as_deref().map(Cow::Borrowed);
    info.srgb = chunks.srgb;
    info.icc_profile = chunks.icc.as_deref().map(Cow::Borrowed);
    info.source_gamma = chunks.gamma;
    info.source_chromaticities = chunks.chromaticities;
    info.pixel_dims = chunks.pixel_dims;
    info.exif_metadata = chunks.exif.as_deref().map(Cow::Borrowed);

    let mut out = Vec::new();
    let mut encoder = png::Encoder::with_info(&mut out, info).map_err(|_| Error::Encoding)?;
    encoder.set_deflate_compression(DeflateCompression::Level(BEST_DEFLATE_LEVEL));
    encoder.set_filter(filter);
    let mut writer = encoder.write_header().map_err(|_| Error::Encoding)?;
    writer.write_image_data(&layout.packed_rows).map_err(|_| Error::Encoding)?;
    writer.finish().map_err(|_| Error::Encoding)?;
    Ok(out)
}

fn bit_depth(depth: u8) -> BitDepth {
    match depth {
        1 => BitDepth::One,
        2 => BitDepth::Two,
        4 => BitDepth::Four,
        8 => BitDepth::Eight,
        _ => BitDepth::Sixteen,
    }
}
