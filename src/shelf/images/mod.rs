//! Images: which files count as one, decoding, encoding, Convert and Compress. On macOS, HEIC and AVIF go through sips.

mod decode;
mod encode;
#[cfg(target_os = "macos")]
mod sips_macos;
#[cfg(not(target_os = "macos"))]
mod sips_unavailable;
mod transform;

#[cfg(target_os = "macos")]
use sips_macos as sips;
#[cfg(not(target_os = "macos"))]
use sips_unavailable as sips;

pub(super) use decode::{Decoded, decode, dimensions, thumbnail};
pub(super) use transform::{compress, convert};

const LIBRARY_DECODED: [&str; 9] = ["png", "jpg", "jpeg", "webp", "tif", "tiff", "bmp", "gif", "ico"];
const SYSTEM_DECODED: [&str; 3] = ["heic", "heif", "avif"];
const COMPRESSIBLE: [&str; 6] = ["png", "jpg", "jpeg", "tif", "tiff", "bmp"];

pub(super) fn is_image_ext(ext: &str) -> bool {
    LIBRARY_DECODED.contains(&ext.to_ascii_lowercase().as_str()) || needs_system_decoder(ext)
}

pub(super) fn is_compressible_ext(ext: &str) -> bool {
    COMPRESSIBLE.contains(&ext.to_ascii_lowercase().as_str()) || needs_system_decoder(ext)
}

fn needs_system_decoder(ext: &str) -> bool {
    cfg!(target_os = "macos") && SYSTEM_DECODED.contains(&ext.to_ascii_lowercase().as_str())
}
