//! Encodes an image in each format, reshaping the pixels where a format needs it: JPEG goes on white, icons fit 256,
//! and profiles go along where the format can carry them.

use std::borrow::Cow;
use std::io::Cursor;

use image::codecs::bmp::BmpEncoder;
use image::codecs::gif::GifEncoder;
use image::codecs::ico::IcoEncoder;
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::{CompressionType, FilterType as PngFilter, PngEncoder};
use image::codecs::tiff::TiffEncoder;
use image::codecs::webp::WebPEncoder;
use image::error::{ImageFormatHint, UnsupportedError};
use image::imageops::FilterType;
use image::{DynamicImage, ImageEncoder, ImageError, ImageResult, RgbImage};

use crate::shelf::{Format, icc};

const ICON_MAX_SIDE: u32 = 256;

pub(super) fn encode(
    image: &DynamicImage,
    icc: Option<&[u8]>,
    format: Format,
    jpeg_quality: u8,
) -> ImageResult<Vec<u8>> {
    let mut out = Cursor::new(Vec::new());
    match format {
        Format::Jpeg => {
            write_with_icc(&flattened_on_white(image), JpegEncoder::new_with_quality(&mut out, jpeg_quality), icc)?
        }
        Format::Png => write_with_icc(
            &without_floats(image),
            PngEncoder::new_with_quality(&mut out, CompressionType::Default, PngFilter::Adaptive),
            icc,
        )?,
        Format::Webp => write_with_icc(&eight_bit_color(image), WebPEncoder::new_lossless(&mut out), icc)?,
        Format::Tiff => write_with_icc(&without_gray_alpha(image), TiffEncoder::new(&mut out), icc)?,
        Format::Bmp => eight_bit(image).write_with_encoder(BmpEncoder::new(&mut out))?,
        Format::Gif => rgba8(image).write_with_encoder(GifEncoder::new(&mut out))?,
        Format::Ico => rgba8(&fitted_icon(image)).write_with_encoder(IcoEncoder::new(&mut out))?,
        Format::Pdf => {
            return Err(ImageError::Unsupported(UnsupportedError::from(ImageFormatHint::Name("PDF".into()))));
        }
    }
    Ok(out.into_inner())
}

fn write_with_icc(image: &DynamicImage, mut encoder: impl ImageEncoder, icc: Option<&[u8]>) -> ImageResult<()> {
    if let Some(profile) = icc.filter(|profile| icc::matches(profile, image.color().has_color())) {
        let _unsupported_by_format = encoder.set_icc_profile(profile.to_vec());
    }
    image.write_with_encoder(encoder)
}

fn flattened_on_white(image: &DynamicImage) -> DynamicImage {
    let gray = !image.color().has_color();
    if !image.color().has_alpha() {
        return if gray {
            DynamicImage::ImageLuma8(image.to_luma8())
        } else {
            DynamicImage::ImageRgb8(image.to_rgb8())
        };
    }
    let rgba = image.to_rgba8();
    let mut rgb = RgbImage::new(rgba.width(), rgba.height());
    for (source, target) in rgba.pixels().zip(rgb.pixels_mut()) {
        let alpha = u32::from(source[3]);
        let over_white = |channel: u8| ((u32::from(channel) * alpha + 255 * (255 - alpha) + 127) / 255) as u8;
        target.0 = [over_white(source[0]), over_white(source[1]), over_white(source[2])];
    }
    let rgb = DynamicImage::ImageRgb8(rgb);
    if gray { DynamicImage::ImageLuma8(rgb.to_luma8()) } else { rgb }
}

fn eight_bit(image: &DynamicImage) -> Cow<'_, DynamicImage> {
    use DynamicImage as D;
    match image {
        D::ImageLuma8(_) | D::ImageLumaA8(_) | D::ImageRgb8(_) | D::ImageRgba8(_) => Cow::Borrowed(image),
        D::ImageLuma16(_) => Cow::Owned(D::ImageLuma8(image.to_luma8())),
        D::ImageLumaA16(_) => Cow::Owned(D::ImageLumaA8(image.to_luma_alpha8())),
        _ if image.color().has_alpha() => Cow::Owned(D::ImageRgba8(image.to_rgba8())),
        _ => Cow::Owned(D::ImageRgb8(image.to_rgb8())),
    }
}

fn eight_bit_color(image: &DynamicImage) -> Cow<'_, DynamicImage> {
    use DynamicImage as D;
    match image {
        D::ImageRgb8(_) | D::ImageRgba8(_) => Cow::Borrowed(image),
        _ if image.color().has_alpha() => Cow::Owned(D::ImageRgba8(image.to_rgba8())),
        _ => Cow::Owned(D::ImageRgb8(image.to_rgb8())),
    }
}

fn without_floats(image: &DynamicImage) -> Cow<'_, DynamicImage> {
    use DynamicImage as D;
    match image {
        D::ImageRgb32F(_) => Cow::Owned(D::ImageRgb16(image.to_rgb16())),
        D::ImageRgba32F(_) => Cow::Owned(D::ImageRgba16(image.to_rgba16())),
        _ => Cow::Borrowed(image),
    }
}

fn without_gray_alpha(image: &DynamicImage) -> Cow<'_, DynamicImage> {
    use DynamicImage as D;
    match image {
        D::ImageLumaA8(_) => Cow::Owned(D::ImageRgba8(image.to_rgba8())),
        D::ImageLumaA16(_) => Cow::Owned(D::ImageRgba16(image.to_rgba16())),
        _ => Cow::Borrowed(image),
    }
}

fn rgba8(image: &DynamicImage) -> DynamicImage {
    DynamicImage::ImageRgba8(image.to_rgba8())
}

fn fitted_icon(image: &DynamicImage) -> Cow<'_, DynamicImage> {
    if image.width() > ICON_MAX_SIDE || image.height() > ICON_MAX_SIDE {
        Cow::Owned(image.resize(ICON_MAX_SIDE, ICON_MAX_SIDE, FilterType::Lanczos3))
    } else {
        Cow::Borrowed(image)
    }
}
