//! Convert and Compress for images: JPEGs get smaller, PNGs are optimized losslessly, and BMP and TIFF become PNG.

use std::path::Path;

use super::decode::{Decoded, decode};
use super::encode::encode;
use super::needs_system_decoder;
use crate::shelf::names::{couldnt_read, file_name, lowercase_extension, too_large};
use crate::shelf::{Format, png_opt};

const CONVERT_JPEG_QUALITY: u8 = 90;
const COMPRESS_JPEG_QUALITY: u8 = 78;

pub(in crate::shelf) fn convert(path: &Path, format: Format) -> Result<Vec<u8>, String> {
    let Decoded { image, icc } = decode(path)?;
    encode(&image, icc.as_deref(), format, CONVERT_JPEG_QUALITY)
        .map_err(|_| format!("Couldn't convert {}", file_name(path)))
}

pub(in crate::shelf) fn compress(path: &Path) -> Result<(Vec<u8>, String), String> {
    let ext = lowercase_extension(path);
    match ext.as_str() {
        "jpg" | "jpeg" => Ok((smaller_jpeg(path)?, ext)),
        "png" => Ok((optimized_png(path)?, ext)),
        "bmp" | "tif" | "tiff" => Ok((lossless_png(path)?, Format::Png.extension().to_owned())),
        _ if needs_system_decoder(&ext) => Ok((smaller_jpeg(path)?, Format::Jpeg.extension().to_owned())),
        _ => Err(format!("Can't compress {}", file_name(path))),
    }
}

fn couldnt_compress(path: &Path) -> String {
    format!("Couldn't compress {}", file_name(path))
}

fn smaller_jpeg(path: &Path) -> Result<Vec<u8>, String> {
    let Decoded { image, icc } = decode(path)?;
    encode(&image, icc.as_deref(), Format::Jpeg, COMPRESS_JPEG_QUALITY).map_err(|_| couldnt_compress(path))
}

fn lossless_png(path: &Path) -> Result<Vec<u8>, String> {
    let Decoded { image, icc } = decode(path)?;
    png_opt::encode_image(&image, icc).map_err(|_| couldnt_compress(path))
}

fn optimized_png(path: &Path) -> Result<Vec<u8>, String> {
    let original = std::fs::read(path).map_err(|_| couldnt_read(path))?;
    png_opt::optimize(&original).map_err(|err| match err {
        png_opt::Error::Unreadable => couldnt_read(path),
        png_opt::Error::TooLarge => too_large(path),
        png_opt::Error::Animated => format!("Can't compress animated {}", file_name(path)),
        png_opt::Error::Unsupported | png_opt::Error::Encoding => couldnt_compress(path),
    })
}
