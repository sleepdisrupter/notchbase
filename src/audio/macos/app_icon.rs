//! App and file icons from AppKit, drawn into 64 pixel images.

use std::sync::Arc;

use egui::ColorImage;
use objc2::rc::Retained;
use objc2_app_kit::NSRunningApplication;
use objc2_core_foundation::{CGFloat, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGColorSpace, CGContext, CGImage, CGImageAlphaInfo, CGImageByteOrderInfo, kCGColorSpaceSRGB,
};

const ICON_PX: usize = 64;
const ROW_BYTES: usize = ICON_PX * 4;
const BITS_PER_COMPONENT: usize = 8;

pub fn app_icon(app: &NSRunningApplication) -> Option<Arc<ColorImage>> {
    let image = icon_image(app)?;
    let rgba = rasterize(&image)?;
    Some(Arc::new(ColorImage::from_rgba_premultiplied([ICON_PX, ICON_PX], &rgba)))
}

pub fn file_icon(path: &std::path::Path) -> Option<Arc<ColorImage>> {
    objc2::rc::autoreleasepool(|_| {
        let path = objc2_foundation::NSString::from_str(&path.to_string_lossy());
        let icon = objc2_app_kit::NSWorkspace::sharedWorkspace().iconForFile(&path);
        let mut proposed = icon_rect();
        let image = unsafe { icon.CGImageForProposedRect_context_hints(&mut proposed, None, None) }?;
        let rgba = rasterize(&image)?;
        Some(Arc::new(ColorImage::from_rgba_premultiplied([ICON_PX, ICON_PX], &rgba)))
    })
}

fn icon_image(app: &NSRunningApplication) -> Option<Retained<CGImage>> {
    let icon = app.icon()?;
    let mut proposed = icon_rect();
    unsafe { icon.CGImageForProposedRect_context_hints(&mut proposed, None, None) }
}

fn rasterize(image: &CGImage) -> Option<Vec<u8>> {
    let space = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }))?;
    let mut rgba = vec![0u8; ICON_PX * ROW_BYTES];
    let data = rgba.as_mut_ptr().cast();
    let info = CGImageAlphaInfo::PremultipliedLast.0 | CGImageByteOrderInfo::Order32Big.0;
    let context =
        unsafe { CGBitmapContextCreate(data, ICON_PX, ICON_PX, BITS_PER_COMPONENT, ROW_BYTES, Some(&space), info) }?;
    CGContext::draw_image(Some(&context), icon_rect(), Some(image));
    drop(context);
    Some(rgba)
}

fn icon_rect() -> CGRect {
    let side = ICON_PX as CGFloat;
    CGRect::new(CGPoint::ZERO, CGSize::new(side, side))
}
