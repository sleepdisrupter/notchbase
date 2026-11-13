//! Draws a CGImage into an opaque sRGB RGBA buffer.

use objc2_core_foundation::{CGFloat, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGColorSpace, CGContext, CGImage, CGImageAlphaInfo, CGImageByteOrderInfo,
    CGInterpolationQuality, kCGColorSpaceSRGB,
};

const RGBA_IN_MEMORY_ORDER: u32 = CGImageAlphaInfo::PremultipliedLast.0 | CGImageByteOrderInfo::Order32Big.0;

pub(super) fn opaque_rgba(image: &CGImage, width: usize, height: usize) -> Option<Vec<u8>> {
    let color_space = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }))?;
    let mut rgba = vec![0u8; width * height * 4];
    let context = unsafe {
        CGBitmapContextCreate(
            rgba.as_mut_ptr().cast(),
            width,
            height,
            8,
            width * 4,
            Some(&color_space),
            RGBA_IN_MEMORY_ORDER,
        )
    }?;
    let whole = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(width as CGFloat, height as CGFloat));
    CGContext::set_rgb_fill_color(Some(&context), 0.0, 0.0, 0.0, 1.0);
    CGContext::fill_rect(Some(&context), whole);
    CGContext::set_interpolation_quality(Some(&context), CGInterpolationQuality::High);
    CGContext::draw_image(Some(&context), whole, Some(image));
    drop(context);
    make_opaque(&mut rgba);
    Some(rgba)
}

fn make_opaque(rgba: &mut [u8]) {
    rgba.as_chunks_mut::<4>().0.iter_mut().for_each(|pixel| pixel[3] = u8::MAX);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_pixel_ends_fully_opaque() {
        let mut rgba = vec![10, 20, 30, 0, 40, 50, 60, 128];
        make_opaque(&mut rgba);
        assert_eq!(rgba, vec![10, 20, 30, 255, 40, 50, 60, 255]);
    }
}
