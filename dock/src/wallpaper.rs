//! The glass's stand-in for Mission Control. There the system can't blur what's behind the dock and shows flat grey,
//! so the dock draws its own: the desktop picture, cropped to the dock's window as the screen shows it, scaled down
//! and blurred like the live blur. It's built ahead of time on a thread of its own, whenever the picture, its file or
//! the window changes, so it's ready the moment Mission Control opens.

use std::path::PathBuf;
use std::time::SystemTime;

use objc2::AnyThread as _;
use objc2::MainThreadMarker;
use objc2::rc::autoreleasepool;
use objc2_app_kit::{NSImage, NSScreen, NSWorkspace};
use objc2_core_foundation::{CGFloat, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGColorSpace, CGContext, CGImage, CGImageAlphaInfo, CGImageByteOrderInfo,
    CGInterpolationQuality, kCGColorSpaceSRGB,
};
use objc2_foundation::{NSNumber, NSRect, NSString};

/// Points per pixel of the picture: the blur hides any detail finer than this.
const STEP: f64 = 4.0;
/// The box blur's reach in pixels; three passes of it come close to the live blur's 24 point Gaussian.
const BLUR_PX: usize = 5;
/// The live blur keeps a little more colour than a plain average does.
const SATURATION: f32 = 1.15;

/// Which picture to draw and where the window sits: when this changes, the picture is built again.
#[derive(Clone, Debug, PartialEq)]
pub struct Source {
    path: PathBuf,
    modified: Option<SystemTime>,
    /// Shown whole inside the screen (with black around it) rather than filling it.
    fit: bool,
    /// The primary screen's size, in points.
    screen: [f64; 2],
    /// The dock's window: its top left from the primary screen's top left, and its size, in points.
    window: [f64; 4],
}

/// The blurred desktop behind the window, premultiplied RGBA, its first row the window's top.
pub struct Picture {
    pub rgba: Vec<u8>,
    pub size: [usize; 2],
}

/// Display `display`'s desktop picture now (the main screen's if it's not found), for the window at `window` (top
/// left x, y and size, in points from that screen's top left). Main thread only.
pub fn source(display: u32, window: [f64; 4]) -> Option<Source> {
    let mtm = MainThreadMarker::new()?;
    autoreleasepool(|_| {
        let screens = NSScreen::screens(mtm);
        let number = |s: &NSScreen| {
            s.deviceDescription()
                .objectForKey(&NSString::from_str("NSScreenNumber"))
                .and_then(|n| n.downcast::<NSNumber>().ok())
                .map(|n| n.unsignedIntValue())
        };
        let screen = screens.iter().find(|s| number(s) == Some(display)).or_else(|| screens.firstObject())?;
        let workspace = NSWorkspace::sharedWorkspace();
        let url = workspace.desktopImageURLForScreen(&screen)?;
        let path = PathBuf::from(url.path()?.to_string());
        let options = workspace.desktopImageOptionsForScreen(&screen);
        let clips = options
            .as_ref()
            .and_then(|o| o.objectForKey(&NSString::from_str("NSWorkspaceDesktopImageAllowClippingKey")))
            .and_then(|v| v.downcast::<NSNumber>().ok())
            .is_none_or(|n| n.boolValue());
        let modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        let size = screen.frame().size;
        Some(Source { path, modified, fit: !clips, screen: [size.width, size.height], window })
    })
}

/// Draws and blurs the picture for `source`. Slow (it decodes the whole file), so it runs off the main thread.
pub fn render(source: &Source) -> Option<Picture> {
    const RGBA: u32 = CGImageAlphaInfo::PremultipliedLast.0 | CGImageByteOrderInfo::Order32Big.0;
    let [wx, wy, ww, wh] = source.window;
    let (w, h) = ((ww / STEP).ceil() as usize, (wh / STEP).ceil() as usize);
    if w == 0 || h == 0 {
        return None;
    }
    let mut rgba = vec![0u8; w * h * 4];
    autoreleasepool(|_| {
        let image =
            NSImage::initWithContentsOfFile(NSImage::alloc(), &NSString::from_str(&source.path.to_string_lossy()))?;
        let cg = unsafe { image.CGImageForProposedRect_context_hints(std::ptr::null_mut::<NSRect>(), None, None) }?;
        let (iw, ih) = (CGImage::width(Some(&cg)) as f64, CGImage::height(Some(&cg)) as f64);
        if iw <= 0.0 || ih <= 0.0 {
            return None;
        }
        let space = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }))?;
        let context = unsafe { CGBitmapContextCreate(rgba.as_mut_ptr().cast(), w, h, 8, w * 4, Some(&space), RGBA) }?;
        CGContext::set_interpolation_quality(Some(&context), CGInterpolationQuality::High);
        CGContext::set_rgb_fill_color(Some(&context), 0.0, 0.0, 0.0, 1.0);
        CGContext::fill_rect(Some(&context), CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(w as f64, h as f64)));
        let [x, y, dw, dh] = placed(source.screen, [iw, ih], source.fit);
        // In the context y runs up from the window's bottom; the screen's from its own bottom.
        let bottom = source.screen[1] - (wy + wh);
        let rect = CGRect::new(
            CGPoint::new((x - wx) / STEP, (y - bottom) / STEP),
            CGSize::new(dw / STEP as CGFloat, dh / STEP as CGFloat),
        );
        CGContext::draw_image(Some(&context), rect, Some(&cg));
        Some(())
    })?;
    blur(&mut rgba, w, h);
    saturate(&mut rgba, SATURATION);
    Some(Picture { rgba, size: [w, h] })
}

/// Where the picture lands on a `screen` of that size, from its bottom left, in points: filling it (cropping the
/// overflow, centred), as the desktop does by default, or fitting inside it.
fn placed(screen: [f64; 2], image: [f64; 2], fit: bool) -> [f64; 4] {
    let (kx, ky) = (screen[0] / image[0], screen[1] / image[1]);
    let k = if fit { kx.min(ky) } else { kx.max(ky) };
    let (dw, dh) = (image[0] * k, image[1] * k);
    [(screen[0] - dw) * 0.5, (screen[1] - dh) * 0.5, dw, dh]
}

/// Three box blurs each way: close to a Gaussian, and cheap on a picture this small. Edges repeat outward, as the
/// live blur's do at the screen's edge.
fn blur(rgba: &mut [u8], w: usize, h: usize) {
    let mut a: Vec<f32> = rgba.iter().map(|&v| f32::from(v)).collect();
    let mut b = a.clone();
    for _ in 0..3 {
        pass(&a, &mut b, w, h, 1, w * 4);
        pass(&b, &mut a, h, w, w, 4);
    }
    for (out, v) in rgba.iter_mut().zip(&a) {
        *out = v.round().clamp(0.0, 255.0) as u8;
    }
}

/// One box blur along lines of `len` pixels `step` apart (in pixels), `count` lines `stride` bytes apart.
fn pass(from: &[f32], to: &mut [f32], len: usize, count: usize, step: usize, stride: usize) {
    let r = BLUR_PX as isize;
    let n = (2 * r + 1) as f32;
    let step = step * 4;
    for line in 0..count {
        let base = line * stride;
        for c in 0..4 {
            let at = |i: isize| from[base + i.clamp(0, len as isize - 1) as usize * step + c];
            let mut sum: f32 = (-r..=r).map(at).sum();
            for i in 0..len as isize {
                to[base + i as usize * step + c] = sum / n;
                sum += at(i + r + 1) - at(i - r);
            }
        }
    }
}

fn saturate(rgba: &mut [u8], amount: f32) {
    for px in rgba.as_chunks_mut::<4>().0 {
        let [r, g, b] = [px[0], px[1], px[2]].map(f32::from);
        let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        let alpha = f32::from(px[3]);
        for (out, v) in px.iter_mut().zip([r, g, b]) {
            *out = (luma + (v - luma) * amount).round().clamp(0.0, alpha) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_picture_fills_the_screen_centred_or_fits_inside_it() {
        // A 2:1 picture on a 3:2 screen: filling crops its sides, fitting leaves bands above and below.
        let [x, y, w, h] = placed([1500.0, 1000.0], [4000.0, 2000.0], false);
        assert_eq!([x, y, w, h], [-250.0, 0.0, 2000.0, 1000.0]);
        let [x, y, w, h] = placed([1500.0, 1000.0], [4000.0, 2000.0], true);
        assert_eq!([x, y, w, h], [0.0, 125.0, 1500.0, 750.0]);
    }

    #[test]
    fn blurring_keeps_a_flat_colour_and_softens_an_edge() {
        let (w, h) = (64, 6);
        let mut rgba: Vec<u8> =
            (0..w * h).flat_map(|i| if i % w < w / 2 { [200, 100, 50, 255] } else { [0, 0, 0, 255] }).collect();
        blur(&mut rgba, w, h);
        let at = |x: usize| rgba[(2 * w + x) * 4];
        assert_eq!(at(0), 200, "far from the edge the colour stays");
        assert_eq!(rgba[3], 255, "and it stays opaque");
        assert!(at(w / 2 - 1) < 160 && at(w / 2) > 40, "the edge is soft: {} {}", at(w / 2 - 1), at(w / 2));
        assert!(at(w / 2 - 1) > at(w / 2), "and still runs the right way");
    }

    #[test]
    fn saturating_leaves_greys_alone() {
        let mut grey = vec![120, 120, 120, 255, 30, 200, 90, 255];
        saturate(&mut grey, 1.5);
        assert_eq!(&grey[..4], &[120, 120, 120, 255]);
        assert!(grey[5] > 200 && grey[4] < 30, "colours move away from their grey");
    }
}
