//! Files out of a stack: their pictures (Quick Look thumbnails, else Finder icons) and real file drags to other
//! apps, the way the notch's shelf drags them.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{AnyThread as _, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSApplication, NSDragOperation, NSDraggingContext, NSDraggingItem, NSDraggingSession, NSDraggingSource, NSEvent,
    NSEventModifierFlags, NSEventType, NSView, NSWindow, NSWorkspace,
};
use objc2_core_foundation::{CGFloat, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGColorSpace, CGContext, CGImage, CGImageAlphaInfo, CGImageByteOrderInfo,
    CGInterpolationQuality, kCGColorSpaceSRGB,
};
use objc2_foundation::{NSArray, NSError, NSPoint, NSProcessInfo, NSRect, NSSize, NSString, NSURL};
use objc2_quick_look_thumbnailing::{
    QLThumbnailGenerationRequest, QLThumbnailGenerationRequestRepresentationTypes, QLThumbnailGenerator,
    QLThumbnailRepresentation, QLThumbnailRepresentationType,
};
use winit::window::Window;

/// A thumbnail that hasn't come in by then isn't waited for: the icon stands in.
const QUICK_LOOK_TIMEOUT: Duration = Duration::from_millis(1500);
const DRAG_ICON_POINTS: CGFloat = 64.0;

/// A picture of `path` at most `px` pixels on its longer side, and whether it's a thumbnail of the contents
/// (rather than an icon). Any thread.
pub fn thumbnail(path: &Path, px: u32) -> Option<(egui::ColorImage, bool)> {
    // Quick Look reads what a link points to, not the link.
    let real = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let ns = NSString::from_str(real.to_str()?);
    if let Some(image) = quick_look(&ns, px) {
        return fitted(&image, px as usize).map(|i| (i, true));
    }
    crate::mac::icon_for_path(path, px).map(|i| (i, false))
}

/// Quick Look's thumbnail, only when it is one: its icons are left to Finder's own, which look the same as
/// everywhere else in the dock.
fn quick_look(path: &NSString, px: u32) -> Option<Retained<CGImage>> {
    let side = CGFloat::from(px as u16);
    let request = unsafe {
        QLThumbnailGenerationRequest::initWithFileAtURL_size_scale_representationTypes(
            QLThumbnailGenerationRequest::alloc(),
            &NSURL::fileURLWithPath(path),
            CGSize::new(side, side),
            1.0,
            QLThumbnailGenerationRequestRepresentationTypes::Thumbnail,
        )
    };
    let (sender, receiver) = mpsc::channel();
    let on_done = RcBlock::new(move |representation: *mut QLThumbnailRepresentation, _error: *mut NSError| {
        let image = unsafe { Retained::retain(representation) }
            .filter(|r| unsafe { r.r#type() } == QLThumbnailRepresentationType::Thumbnail)
            .map(|r| unsafe { r.CGImage() });
        let _ = sender.send(image);
    });
    let generator = unsafe { QLThumbnailGenerator::sharedGenerator() };
    unsafe { generator.generateBestRepresentationForRequest_completionHandler(&request, &on_done) };
    match receiver.recv_timeout(QUICK_LOOK_TIMEOUT) {
        Ok(image) => image,
        Err(_) => {
            unsafe { generator.cancelRequest(&request) };
            None
        }
    }
}

/// Draws `image` into premultiplied sRGB pixels, its longer side `most` pixels, keeping its shape.
fn fitted(image: &CGImage, most: usize) -> Option<egui::ColorImage> {
    const RGBA: u32 = CGImageAlphaInfo::PremultipliedLast.0 | CGImageByteOrderInfo::Order32Big.0;
    let (w, h) = (CGImage::width(Some(image)), CGImage::height(Some(image)));
    if w == 0 || h == 0 || most == 0 {
        return None;
    }
    let k = (most as f64 / w.max(h) as f64).min(1.0);
    let (w, h) = (((w as f64 * k).round() as usize).max(1), ((h as f64 * k).round() as usize).max(1));
    let space = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }))?;
    let mut rgba = vec![0u8; w * h * 4];
    let context = unsafe { CGBitmapContextCreate(rgba.as_mut_ptr().cast(), w, h, 8, w * 4, Some(&space), RGBA) }?;
    CGContext::set_interpolation_quality(Some(&context), CGInterpolationQuality::High);
    let all = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(w as CGFloat, h as CGFloat));
    CGContext::draw_image(Some(&context), all, Some(image));
    drop(context);
    Some(egui::ColorImage::from_rgba_premultiplied([w, h], &rgba))
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "NotchbaseDockFileDragSource"]
    struct DragSource;

    unsafe impl NSObjectProtocol for DragSource {}

    unsafe impl NSDraggingSource for DragSource {
        #[unsafe(method(draggingSession:sourceOperationMaskForDraggingContext:))]
        fn allowed_operations(&self, _session: &NSDraggingSession, context: NSDraggingContext) -> NSDragOperation {
            // Other apps get a copy (Finder moves it, as from any folder); the dock itself takes nothing.
            if context == NSDraggingContext::OutsideApplication {
                NSDragOperation::Copy | NSDragOperation::Move | NSDragOperation::Generic
            } else {
                NSDragOperation::None
            }
        }

        #[unsafe(method(draggingSession:endedAtPoint:operation:))]
        fn session_ended(&self, _session: &NSDraggingSession, _point: NSPoint, _operation: NSDragOperation) {
            DRAGGING.store(false, Ordering::Relaxed);
            ENDED.store(true, Ordering::Relaxed);
        }
    }
);

/// A drag is under way, and one has ended since the app last looked.
static DRAGGING: AtomicBool = AtomicBool::new(false);
static ENDED: AtomicBool = AtomicBool::new(false);

/// Whether files are being dragged out of the dock right now.
pub fn dragging() -> bool {
    DRAGGING.load(Ordering::Relaxed)
}

/// Whether a drag ended since the last call.
pub fn take_ended() -> bool {
    ENDED.swap(false, Ordering::Relaxed)
}

thread_local! {
    /// The drag's source, kept alive while the drag runs.
    static SOURCE: std::cell::RefCell<Option<Retained<DragSource>>> = const { std::cell::RefCell::new(None) };
}

/// Starts a system drag of `paths` from the dock's window, each with its Finder icon under the pointer. Main thread.
pub fn drag(window: &Window, paths: &[PathBuf]) -> Result<(), String> {
    let mtm = MainThreadMarker::new().ok_or("Drags start on the main thread")?;
    let ns_window = crate::mac::ns_window(window).ok_or("No window to drag from")?;
    let view: Retained<NSView> = ns_window.contentView().ok_or("No view to drag from")?;
    let event = mouse_event(mtm, &ns_window).ok_or("No mouse event to drag from")?;
    let cursor = view.convertPoint_fromView(event.locationInWindow(), None);
    let half = DRAG_ICON_POINTS / 2.0;
    let frame =
        NSRect::new(NSPoint::new(cursor.x - half, cursor.y - half), NSSize::new(DRAG_ICON_POINTS, DRAG_ICON_POINTS));
    let workspace = NSWorkspace::sharedWorkspace();
    let items: Vec<Retained<NSDraggingItem>> = paths
        .iter()
        .filter_map(|path| {
            let ns = NSString::from_str(path.to_str()?);
            let url = NSURL::fileURLWithPath(&ns);
            let item =
                NSDraggingItem::initWithPasteboardWriter(NSDraggingItem::alloc(), ProtocolObject::from_ref(&*url));
            let icon = workspace.iconForFile(&ns);
            icon.setSize(frame.size);
            let contents: &AnyObject = &icon;
            unsafe { item.setDraggingFrame_contents(frame, Some(contents)) };
            Some(item)
        })
        .collect();
    if items.is_empty() {
        return Err("Nothing to drag".into());
    }
    let source: Retained<DragSource> = unsafe { msg_send![super(DragSource::alloc(mtm).set_ivars(())), init] };
    let session = view.beginDraggingSessionWithItems_event_source(
        &NSArray::from_retained_slice(&items),
        &event,
        ProtocolObject::from_ref(&*source),
    );
    session.setAnimatesToStartingPositionsOnCancelOrFail(true);
    SOURCE.set(Some(source));
    DRAGGING.store(true, Ordering::Relaxed);
    Ok(())
}

/// The press or drag the drag starts from: the current one when it's in this window, else one made up here.
fn mouse_event(mtm: MainThreadMarker, window: &NSWindow) -> Option<Retained<NSEvent>> {
    let current = NSApplication::sharedApplication(mtm).currentEvent().filter(|e| {
        matches!(e.r#type(), NSEventType::LeftMouseDown | NSEventType::LeftMouseDragged)
            && e.windowNumber() == window.windowNumber()
    });
    current.or_else(|| {
        NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
            NSEventType::LeftMouseDragged,
            window.convertPointFromScreen(NSEvent::mouseLocation()),
            NSEventModifierFlags::empty(),
            NSProcessInfo::processInfo().systemUptime(),
            window.windowNumber(),
            None,
            0,
            1,
            1.0,
        )
    })
}
