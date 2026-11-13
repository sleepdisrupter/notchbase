//! Reads a PNG into plain pixels and the chunks that change how it renders.

use std::io::Cursor;

use png::{BitDepth, ColorType, Info, Transformations};

use super::pixels::Pixels;
use super::{Error, RenderingChunks};
use crate::shelf::MAX_DECODED_BYTES;

pub(super) fn decode(png_file: &[u8]) -> Result<(Pixels, RenderingChunks), Error> {
    let budget = usize::try_from(MAX_DECODED_BYTES).unwrap_or(usize::MAX);
    let mut decoder = png::Decoder::new_with_limits(Cursor::new(png_file), png::Limits { bytes: budget });
    decoder.set_transformations(Transformations::EXPAND);
    let mut reader = decoder.read_info().map_err(|_| Error::Unreadable)?;
    let chunks = rendering_chunks(reader.info())?;
    let buffer_size = reader.output_buffer_size().ok_or(Error::TooLarge)?;
    if buffer_size > budget {
        return Err(Error::TooLarge);
    }
    let mut data = vec![0; buffer_size];
    let frame = reader.next_frame(&mut data).map_err(|_| Error::Unreadable)?;
    data.truncate(frame.buffer_size());
    let pixels = Pixels {
        width: frame.width,
        height: frame.height,
        channels: channel_count(frame.color_type)?,
        sample_bytes: sample_bytes(frame.bit_depth)?,
        data,
    };
    if pixels.data.len() != pixels.len() * pixels.pixel_bytes() {
        return Err(Error::Unreadable);
    }
    Ok((pixels, chunks))
}

fn rendering_chunks(info: &Info<'_>) -> Result<RenderingChunks, Error> {
    if info.animation_control.is_some() {
        return Err(Error::Animated);
    }
    let has_hdr_metadata = info.coding_independent_code_points.is_some()
        || info.mastering_display_color_volume.is_some()
        || info.content_light_level.is_some();
    if has_hdr_metadata {
        return Err(Error::Unsupported);
    }
    Ok(RenderingChunks {
        srgb: info.srgb,
        icc: info.icc_profile.as_ref().map(|profile| profile.to_vec()),
        gamma: info.gama_chunk,
        chromaticities: info.chrm_chunk,
        pixel_dims: info.pixel_dims,
        exif: info.exif_metadata.as_ref().map(|exif| exif.to_vec()),
    })
}

fn channel_count(color: ColorType) -> Result<usize, Error> {
    match color {
        ColorType::Grayscale => Ok(1),
        ColorType::GrayscaleAlpha => Ok(2),
        ColorType::Rgb => Ok(3),
        ColorType::Rgba => Ok(4),
        ColorType::Indexed => Err(Error::Unreadable),
    }
}

fn sample_bytes(depth: BitDepth) -> Result<usize, Error> {
    match depth {
        BitDepth::Eight => Ok(1),
        BitDepth::Sixteen => Ok(2),
        BitDepth::One | BitDepth::Two | BitDepth::Four => Err(Error::Unreadable),
    }
}
