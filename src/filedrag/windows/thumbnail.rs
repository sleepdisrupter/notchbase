//! Thumbnails from the shell, premultiplied and fitted within the size asked for.

use std::path::Path;

use super::super::fit::fit_within;
use super::bitmap::OwnedBitmap;
use super::pixels::{downscale_premultiplied, premultiply_shell_alpha};
use super::shell_image::shell_image;

pub(in crate::filedrag) fn thumbnail(path: &Path, max_px: u32) -> Option<egui::ColorImage> {
    fitted(&shell_image(path, max_px)?, max_px)
}

/// A shell bitmap as an egui image, premultiplied and scaled down to fit `max_px`.
pub(super) fn fitted(bitmap: &OwnedBitmap, max_px: u32) -> Option<egui::ColorImage> {
    let mut pixels = bitmap.read_rgba()?;
    premultiply_shell_alpha(&mut pixels.rgba);
    let (width, height) = fit_within(pixels.width, pixels.height, max_px);
    let rgba = if (width, height) == (pixels.width, pixels.height) {
        pixels.rgba
    } else {
        downscale_premultiplied(&pixels.rgba, pixels.width, pixels.height, width, height)
    };
    Some(egui::ColorImage::from_rgba_premultiplied([width, height], &rgba))
}
