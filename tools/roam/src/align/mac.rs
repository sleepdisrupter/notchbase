//! Line-up guides on a Mac: a clear, borderless window over each screen that lets every click
//! through, drawn with Core Animation layers. AppKit's parts live on the main thread; the
//! engine's thread hands them what to show.

use std::cell::RefCell;
use std::sync::Mutex;

use dispatch2::DispatchQueue;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, MainThreadOnly as _, msg_send};
use objc2_app_kit::{
    NSBackingStoreType, NSColor, NSScreen, NSView, NSWindow, NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_core_foundation::{CFString, CGPoint, CGRect, CGSize};
use objc2_core_graphics::CGColor;
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
use objc2_quartz_core::{CALayer, CATextLayer, CATransaction, kCAAlignmentCenter};

use super::{Guide, Marked, Shape, Shown, mark_scene, scene};
use crate::layout::Rect;

/// Above other apps' windows, even full-screen ones, and just under a lent screen's picture
/// (999), which covers its screen whole.
const LEVEL: isize = 998;

struct Overlay {
    rect: Rect,
    window: Retained<NSWindow>,
    /// The guide's layers, and the marks' above them.
    guide: Retained<CALayer>,
    marks: Retained<CALayer>,
    shown: Guide,
}

thread_local! {
    /// The windows up now: only touched on the main thread.
    static OVERLAYS: RefCell<Vec<Overlay>> = const { RefCell::new(Vec::new()) };
}

/// The newest marks not yet drawn, so a busy main thread draws only the latest.
static MARKS: Mutex<Option<Vec<Marked>>> = Mutex::new(None);

pub fn show(shown: Vec<Shown>) {
    DispatchQueue::main().exec_async(move || {
        let Some(mtm) = MainThreadMarker::new() else { return };
        apply(mtm, shown);
    });
}

pub fn mark(marks: Vec<Marked>) {
    let Ok(mut waiting) = MARKS.lock() else { return };
    let first = waiting.is_none();
    *waiting = Some(marks);
    drop(waiting);
    if first {
        DispatchQueue::main().exec_async(|| {
            let marks = MARKS.lock().ok().and_then(|mut waiting| waiting.take());
            if let Some(marks) = marks {
                draw_marks(&marks);
            }
        });
    }
}

/// On the main thread: windows for `shown`, and none on other screens.
fn apply(mtm: MainThreadMarker, shown: Vec<Shown>) {
    OVERLAYS.with_borrow_mut(|overlays| {
        overlays.retain(|overlay| {
            let keep = shown.iter().any(|s| s.rect == overlay.rect);
            if !keep {
                overlay.window.orderOut(None);
                overlay.window.close();
            }
            keep
        });
        for Shown { rect, guide } in shown {
            match overlays.iter_mut().find(|o| o.rect == rect) {
                Some(overlay) if overlay.shown == guide => {}
                Some(overlay) => {
                    fill(&overlay.guide, rect, &scene([rect.w, rect.h], &guide), scale_of(&overlay.window));
                    overlay.shown = guide;
                }
                None => overlays.push(open(mtm, rect, guide)),
            }
        }
    });
}

fn scale_of(window: &NSWindow) -> f64 {
    window.backingScaleFactor().max(1.0)
}

fn open(mtm: MainThreadMarker, rect: Rect, guide: Guide) -> Overlay {
    // AppKit counts up from the bottom of the first screen.
    let first = NSScreen::screens(mtm).firstObject().map_or(rect.h, |screen| screen.frame().size.height);
    let frame = NSRect::new(NSPoint::new(rect.x, first - rect.y - rect.h), NSSize::new(rect.w, rect.h));
    // SAFETY: NSWindow's own initializer, on the main thread.
    let window: Retained<NSWindow> = unsafe {
        msg_send![
            NSWindow::alloc(mtm),
            initWithContentRect: frame,
            styleMask: NSWindowStyleMask::Borderless,
            backing: NSBackingStoreType::Buffered,
            defer: false,
        ]
    };
    // SAFETY: plain window settings on the main thread.
    unsafe { window.setReleasedWhenClosed(false) };
    window.setLevel(LEVEL);
    window.setOpaque(false);
    window.setHasShadow(false);
    window.setIgnoresMouseEvents(true);
    window.setBackgroundColor(Some(&NSColor::clearColor()));
    window.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::Stationary
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::IgnoresCycle,
    );
    let bounds = CGRect::new(CGPoint::ZERO, CGSize::new(rect.w, rect.h));
    let (root, guide_layer, marks) = (CALayer::layer(), CALayer::layer(), CALayer::layer());
    for layer in [&root, &guide_layer, &marks] {
        layer.setFrame(bounds);
    }
    root.addSublayer(&guide_layer);
    marks.setZPosition(1.0);
    root.addSublayer(&marks);
    let view = NSView::new(mtm);
    view.setLayer(Some(&root));
    view.setWantsLayer(true);
    window.setContentView(Some(&view));
    fill(&guide_layer, rect, &scene([rect.w, rect.h], &guide), scale_of(&window));
    window.orderFrontRegardless();
    Overlay { rect, window, guide: guide_layer, marks, shown: guide }
}

/// On the main thread: each screen's marks, in place of the ones before.
fn draw_marks(marks: &[Marked]) {
    OVERLAYS.with_borrow(|overlays| {
        for overlay in overlays {
            let here: Vec<_> = marks.iter().filter(|m| m.rect == overlay.rect).map(|m| m.mark).collect();
            let shapes = mark_scene([overlay.rect.w, overlay.rect.h], &here);
            fill(&overlay.marks, overlay.rect, &shapes, scale_of(&overlay.window));
        }
    });
}

fn colour([r, g, b, a]: [u8; 4]) -> objc2_core_foundation::CFRetained<CGColor> {
    let part = |v: u8| f64::from(v) / 255.0;
    CGColor::new_srgb(part(r), part(g), part(b), part(a))
}

/// Puts `shapes` (top-down, from the screen's top left) in `layer`, in place of what was there.
fn fill(layer: &CALayer, rect: Rect, shapes: &[Shape], scale: f64) {
    CATransaction::begin();
    CATransaction::setDisableActions(true);
    // SAFETY: no sublayers is a sound value.
    unsafe { layer.setSublayers(None) };
    // Core Animation counts up from the bottom.
    let frame = |[x, y, w, h]: [f64; 4]| CGRect::new(CGPoint::new(x, rect.h - y - h), CGSize::new(w, h));
    for shape in shapes {
        match shape {
            Shape::Rect { at, fill, radius } => {
                let part = CALayer::layer();
                part.setFrame(frame(*at));
                part.setBackgroundColor(Some(&colour(*fill)));
                part.setCornerRadius(*radius);
                layer.addSublayer(&part);
            }
            Shape::Text { at, text, size, colour: ink, bold } => {
                let words = CATextLayer::new();
                // A line of words sits at the top of its layer: one line high, in the middle of the box.
                let line = size * 1.25;
                let [x, y, w, h] = *at;
                words.setFrame(frame([x, y + (h - line) / 2.0 + size * 0.04, w, line]));
                let string = NSString::from_str(text);
                let font = CFString::from_str(if *bold { "HelveticaNeue-Bold" } else { "HelveticaNeue" });
                // SAFETY: a string and a font name are what a text layer takes.
                unsafe {
                    words.setString(Some(&*(Retained::as_ptr(&string).cast::<AnyObject>())));
                    words.setFont(Some(&font));
                    words.setAlignmentMode(kCAAlignmentCenter);
                }
                words.setFontSize(*size);
                words.setForegroundColor(Some(&colour(*ink)));
                words.setContentsScale(scale);
                layer.addSublayer(&words);
            }
        }
    }
    CATransaction::commit();
}
