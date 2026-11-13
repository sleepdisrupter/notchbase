//! Draws a CGImage into premultiplied sRGB pixels, fitted within the size asked for.

use objc2_core_foundation::{CFRetained, CGFloat, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGColorSpace, CGContext, CGImage, CGImageAlphaInfo, CGImageByteOrderInfo,
    CGInterpolationQuality, kCGColorSpaceSRGB,
};

use super::super::fit::fit_within;

const PREMULTIPLIED_RGBA: u32 = CGImageAlphaInfo::PremultipliedLast.0 | CGImageByteOrderInfo::Order32Big.0;

pub(super) fn rasterize_to_fit(image: &CGImage, max_px: u32) -> Option<egui::ColorImage> {
    let (width, height) = (CGImage::width(Some(image)), CGImage::height(Some(image)));
    if width == 0 || height == 0 {
        return None;
    }
    let (width, height) = fit_within(width, height, max_px);
    let rgba = draw_premultiplied(image, width, height)?;
    Some(egui::ColorImage::from_rgba_premultiplied([width, height], &rgba))
}

fn draw_premultiplied(image: &CGImage, width: usize, height: usize) -> Option<Vec<u8>> {
    let color_space = srgb()?;
    let mut rgba = vec![0u8; width * height * 4];
    let context = unsafe {
        CGBitmapContextCreate(
            rgba.as_mut_ptr().cast(),
            width,
            height,
            8,
            width * 4,
            Some(&color_space),
            PREMULTIPLIED_RGBA,
        )
    }?;
    CGContext::set_interpolation_quality(Some(&context), CGInterpolationQuality::High);
    let whole = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(width as CGFloat, height as CGFloat));
    CGContext::draw_image(Some(&context), whole, Some(image));
    drop(context);
    Some(rgba)
}

fn srgb() -> Option<CFRetained<CGColorSpace>> {
    CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }))
}
