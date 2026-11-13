//! Decodes an image upright with its color profile, within the size limit. Also reads dimensions and draws thumbnails.

use std::io::{BufRead, Cursor, Seek};
use std::path::Path;

use egui::ColorImage;
use image::error::{LimitError, LimitErrorKind};
use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder, ImageError, ImageReader, ImageResult};

use super::{needs_system_decoder, sips};
use crate::shelf::MAX_DECODED_BYTES;
use crate::shelf::names::{couldnt_read, lowercase_extension, too_large};

pub(in crate::shelf) struct Decoded {
    pub(in crate::shelf) image: DynamicImage,
    pub(in crate::shelf) icc: Option<Vec<u8>>,
}

pub(in crate::shelf) fn decode(path: &Path) -> Result<Decoded, String> {
    let decoded = if needs_system_decoder(&lowercase_extension(path)) {
        let png = sips::to_png(path, None).ok_or_else(|| couldnt_read(path))?;
        decode_upright(ImageReader::new(Cursor::new(png)))
    } else {
        ImageReader::open(path).map_err(ImageError::from).and_then(decode_upright)
    };
    decoded.map_err(|err| unreadable(path, &err))
}

pub(super) fn unreadable(path: &Path, err: &ImageError) -> String {
    match err {
        ImageError::Limits(_) => too_large(path),
        _ => couldnt_read(path),
    }
}

fn decode_upright<R: BufRead + Seek>(reader: ImageReader<R>) -> ImageResult<Decoded> {
    let mut decoder = reader.with_guessed_format()?.into_decoder()?;
    if decoder.total_bytes() > MAX_DECODED_BYTES {
        return Err(ImageError::Limits(LimitError::from_kind(LimitErrorKind::InsufficientMemory)));
    }
    let icc = decoder.icc_profile().ok().flatten();
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let mut image = DynamicImage::from_decoder(decoder)?;
    image.apply_orientation(orientation);
    Ok(Decoded { image, icc })
}

pub(in crate::shelf) fn dimensions(path: &Path) -> Option<[u32; 2]> {
    if needs_system_decoder(&lowercase_extension(path)) {
        return sips::dimensions(path);
    }
    let mut decoder = ImageReader::open(path).ok()?.with_guessed_format().ok()?.into_decoder().ok()?;
    let (width, height) = decoder.dimensions();
    let quarter_turned = matches!(
        decoder.orientation(),
        Ok(Orientation::Rotate90 | Orientation::Rotate270 | Orientation::Rotate90FlipH | Orientation::Rotate270FlipH)
    );
    Some(if quarter_turned { [height, width] } else { [width, height] })
}

pub(in crate::shelf) fn thumbnail(path: &Path, max_side: u32) -> Option<ColorImage> {
    let image = if needs_system_decoder(&lowercase_extension(path)) {
        let small_png = sips::to_png(path, Some(max_side))?;
        decode_upright(ImageReader::new(Cursor::new(small_png))).ok()?.image
    } else {
        decode(path).ok()?.image
    };
    let fitted =
        if image.width() > max_side || image.height() > max_side { image.thumbnail(max_side, max_side) } else { image };
    let rgba = fitted.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    Some(ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()))
}
