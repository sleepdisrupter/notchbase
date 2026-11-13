//! The real blur behind the dock's glass: the same treatment as the open notch panel (`src/platform/macos/backdrop.rs`
//! in the notchbase app). An NSVisualEffectView sits under the GL surface, masked to the dock's outline, with its own
//! tint layers hidden so only the blur shows; the liquid is drawn over it as tinted glass.

use egui::{Rect, pos2};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, MainThreadOnly as _, msg_send};
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSView, NSVisualEffectBlendingMode, NSVisualEffectMaterial, NSVisualEffectState,
    NSVisualEffectView, NSWindowOrderingMode,
};
use objc2_core_foundation::{CFRetained, CGFloat, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{CGBitmapContextCreate, CGBitmapContextCreateImage, CGImage, CGImageAlphaInfo};
use objc2_foundation::{NSNumber, NSString, ns_string};
use objc2_quartz_core::{CALayer, CATransaction, kCAFilterLinear};
use winit::window::Window;

use crate::liquid::{self, Blob};
use crate::mac;

const BLUR_FILTER: &str = "gaussianBlur";
/// Softer than the notch panel's 8, since the dock is all dark glass: the desktop shows only as a deep blur.
const BLUR_RADIUS: f64 = 24.0;
/// The mask reaches under the keyline, so the blur meets it with no gap.
const UNDER_KEYLINE: f32 = 0.8;

pub struct Backdrop {
    view: Option<(Retained<NSView>, Retained<CALayer>)>,
    shown: Vec<Blob>,
    alpha: f64,
}

impl Backdrop {
    pub fn new(window: &Window) -> Self {
        Self { view: behind(window), shown: Vec::new(), alpha: 1.0 }
    }

    /// Fades the blur with the dock, so it crossfades for Mission Control instead of popping off.
    pub fn fade(&mut self, alpha: f32) {
        let Some((view, _)) = &self.view else { return };
        let alpha = f64::from(alpha.clamp(0.0, 1.0));
        if (alpha - self.alpha).abs() < 0.004 {
            return;
        }
        self.alpha = alpha;
        CATransaction::begin();
        CATransaction::setDisableActions(true);
        view.setAlphaValue(alpha);
        CATransaction::commit();
    }

    /// Masks the blur to the liquid's outline: the dock, its necks and popups as the one melted shape the shader
    /// draws, so the blur fills the fillets too and never shows a seam where they join.
    pub fn shape(&mut self, blobs: &[Blob], _ppp: f32) {
        let Some((view, mask)) = &self.view else { return };
        if let Some(layer) = view.layer() {
            clear_glass(&layer);
        }
        let blobs: Vec<Blob> = blobs.iter().copied().filter(|b| b.half.x > 0.5 && b.half.y > 0.5).collect();
        if blobs == self.shown {
            return;
        }
        let height = view.frame().size.height;
        CATransaction::begin();
        CATransaction::setDisableActions(true);
        match mask_image(&blobs) {
            Some((image, bounds)) => {
                mask.setFrame(CGRect::new(
                    CGPoint::new(bounds.min.x.into(), height - CGFloat::from(bounds.max.y)),
                    CGSize::new(bounds.width().into(), bounds.height().into()),
                ));
                let object = CFRetained::as_ptr(&image).as_ptr() as *const AnyObject;
                unsafe { mask.setContents(Some(&*object)) };
                view.setHidden(false);
            }
            None => view.setHidden(true),
        }
        CATransaction::commit();
        self.shown = blobs;
    }
}

/// How finely the mask follows the outline, in pixels per point. The keyline covers the last fraction of a point.
const MASK_SCALE: f32 = 1.0;

/// The melted outline as an alpha image, and the window area (top left points) it covers.
fn mask_image(blobs: &[Blob]) -> Option<(CFRetained<CGImage>, Rect)> {
    let bounds = blobs.iter().map(Blob::bounds).reduce(Rect::union)?.expand(2.0 + UNDER_KEYLINE);
    let bounds = Rect::from_min_max(bounds.min.floor(), bounds.max.ceil());
    let (w, h) = ((bounds.width() * MASK_SCALE) as usize, (bounds.height() * MASK_SCALE) as usize);
    if w == 0 || h == 0 {
        return None;
    }
    let mut alpha = vec![0u8; w * h];
    let at = |x: f32, y: f32| {
        liquid::distance(blobs, pos2(bounds.min.x + x / MASK_SCALE, bounds.min.y + y / MASK_SCALE)) - UNDER_KEYLINE
    };
    // In blocks: one look at the middle decides a block that's clearly in or out; only blocks on the edge are worked
    // out pixel by pixel. The distance never changes faster than the distance travelled.
    const BLOCK: usize = 8;
    let reach = BLOCK as f32 * 0.75 / MASK_SCALE + 1.0;
    for by in (0..h).step_by(BLOCK) {
        for bx in (0..w).step_by(BLOCK) {
            let (x1, y1) = ((bx + BLOCK).min(w), (by + BLOCK).min(h));
            let d = at((bx + x1) as f32 * 0.5, (by + y1) as f32 * 0.5);
            if d > reach {
                continue;
            }
            for y in by..y1 {
                for x in bx..x1 {
                    alpha[y * w + x] = if d < -reach {
                        255
                    } else {
                        let d = at(x as f32 + 0.5, y as f32 + 0.5) * MASK_SCALE;
                        ((0.5 - d).clamp(0.0, 1.0) * 255.0).round() as u8
                    };
                }
            }
        }
    }
    let context =
        unsafe { CGBitmapContextCreate(alpha.as_mut_ptr().cast(), w, h, 8, w, None, CGImageAlphaInfo::Only.0) }?;
    let image = CGBitmapContextCreateImage(Some(&context))?;
    Some((image, bounds))
}

/// The view's own tint would turn the glass flat gray, so every layer but the blur is hidden.
fn clear_glass(layer: &CALayer) {
    let Some(sublayers) = (unsafe { layer.sublayers() }) else { return };
    for sub in sublayers.iter() {
        let class = unsafe { (*(&*sub as *const CALayer as *const AnyObject)).class() };
        if class.name().to_string_lossy().contains("Backdrop") {
            soften(&sub);
        } else if unsafe { sub.sublayers() }.is_some_and(|inner| inner.count() > 0) {
            clear_glass(&sub);
        } else if sub.opacity() != 0.0 {
            sub.setOpacity(0.0);
        }
    }
}

fn soften(layer: &CALayer) {
    let Some(filters) = layer.filters() else { return };
    let blurs = filters.iter().any(|filter| unsafe {
        let name: Option<Retained<NSString>> = msg_send![&*filter, name];
        name.is_some_and(|name| name.to_string() == BLUR_FILTER)
    });
    if blurs {
        let radius = NSNumber::new_f64(BLUR_RADIUS);
        let _: () =
            unsafe { msg_send![layer, setValue: &*radius, forKeyPath: ns_string!("filters.gaussianBlur.inputRadius")] };
    }
}

fn behind(window: &Window) -> Option<(Retained<NSView>, Retained<CALayer>)> {
    let mtm = MainThreadMarker::new()?;
    let ns_window = mac::ns_window(window)?;
    let content = ns_window.contentView()?;
    let frame: Retained<NSView> = unsafe { content.superview() }?;
    let effect = NSVisualEffectView::initWithFrame(NSVisualEffectView::alloc(mtm), content.frame());
    effect.setMaterial(NSVisualEffectMaterial::HUDWindow);
    effect.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
    effect.setState(NSVisualEffectState::Active);
    let view: Retained<NSView> = Retained::into_super(effect);
    view.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
    );
    view.setWantsLayer(true);
    view.setHidden(true);
    let mask = CALayer::new();
    mask.setMagnificationFilter(unsafe { kCAFilterLinear });
    mask.setMinificationFilter(unsafe { kCAFilterLinear });
    let layer = view.layer()?;
    unsafe { layer.setMask(Some(&mask)) };
    frame.addSubview_positioned_relativeTo(&view, NSWindowOrderingMode::Below, Some(&content));
    Some((view, mask))
}
