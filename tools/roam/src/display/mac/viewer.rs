//! A lent screen: a borderless window above everything on that screen, showing the host's
//! pictures with an `AVSampleBufferDisplayLayer` (which decodes in hardware), and handing what
//! the mouse and keyboard do over it to the host. AppKit's parts live on the main thread;
//! pictures go to the layer from the stream's thread.
//!
//! A host that sends its pointer apart has it drawn here, on a layer above the pictures, so it
//! moves as soon as its news comes rather than with the next picture.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use dispatch2::DispatchQueue;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol};
use objc2::{AllocAnyThread as _, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSApplication, NSBackingStoreType, NSColor, NSEvent, NSEventModifierFlags, NSResponder, NSScreen, NSTrackingArea,
    NSTrackingAreaOptions, NSView, NSWindow, NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_av_foundation::{AVLayerVideoGravityResize, AVQueuedSampleBufferRenderingStatus, AVSampleBufferDisplayLayer};
use objc2_core_foundation::{CFBoolean, CFData, CFMutableDictionary, CFRetained, CFString, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    CGBitmapInfo, CGColorRenderingIntent, CGColorSpace, CGDataProvider, CGImage, CGImageAlphaInfo, kCGColorSpaceSRGB,
};
use objc2_core_media::{
    CMBlockBuffer, CMFormatDescription, CMSampleBuffer, CMVideoFormatDescriptionCreateFromH264ParameterSets,
    kCMBlockBufferAssureMemoryNowFlag, kCMSampleAttachmentKey_DisplayImmediately,
};
use objc2_foundation::{NSPoint, NSRect, NSSize};
use objc2_quartz_core::{CAAutoresizingMask, CALayer, CATransaction};

use crate::display::{Picture, PointerAt, PointerShape, Remote, RemoteSink, Showing, h264};
use crate::keys;
use crate::layout::Rect;

/// Above every other window, even a full-screen app's.
const LEVEL: isize = 1000;
/// How long the main thread may take to put the window up.
const OPEN_WAIT: Duration = Duration::from_secs(3);
/// Trackpad scrolling comes in pixels; this many make one wheel notch (as Roam's own input).
const PIXELS_PER_NOTCH: f64 = 40.0;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGMainDisplayID() -> u32;
    fn CGDisplayHideCursor(display: u32) -> i32;
    fn CGDisplayShowCursor(display: u32) -> i32;
    fn _CGSDefaultConnection() -> i32;
    fn CGSSetConnectionProperty(connection: i32, target: i32, key: *mut c_void, value: *mut c_void) -> i32;
}

/// Something only touched on the main thread, or handed over whole.
struct Sendable<T>(T);
// SAFETY: as above.
unsafe impl<T> Send for Sendable<T> {}
// SAFETY: as above: shared only to be handed to the main thread.
unsafe impl<T> Sync for Sendable<T> {}

pub struct Viewer {
    window: Option<Sendable<Retained<LentWindow>>>,
    layer: Retained<AVSampleBufferDisplayLayer>,
    format: Option<CFRetained<CMFormatDescription>>,
    /// The host's pointer, drawn above the pictures: only touched on the main thread.
    pointer_layer: Arc<Sendable<Retained<CALayer>>>,
    /// What's new about the pointer, for the main thread to draw.
    pointer: Arc<Mutex<PointerState>>,
    /// The size of the pictures, in pixels.
    size: [u32; 2],
}

/// The pointer's news between the stream's thread and the main thread: the newest only, and
/// one drawing on its way at a time, so a busy main thread never draws stale places.
#[derive(Default)]
struct PointerState {
    at: Option<PointerAt>,
    /// A shape not yet drawn, and the size of the one drawn.
    shape: Option<PointerShape>,
    shape_size: Option<[u32; 2]>,
    /// When the news on its way to the main thread came.
    pending: Option<Instant>,
    /// The longest news waited to be drawn since the last look.
    longest: Option<Duration>,
}

// SAFETY: the layer takes sample buffers from any thread; the window is only passed back to
// the main thread to close.
unsafe impl Send for Viewer {}

impl Viewer {
    pub fn open(rect: Rect, size: [u32; 2], remote: RemoteSink) -> Result<Self, String> {
        let (made, wait) = mpsc::channel();
        DispatchQueue::main().exec_async(move || {
            let Some(mtm) = MainThreadMarker::new() else { return };
            let _ = made.send(Sendable(make_window(mtm, rect, remote)));
        });
        let Ok(Sendable((window, layer, pointer_layer))) = wait.recv_timeout(OPEN_WAIT) else {
            return Err("The window for the picture didn't open".into());
        };
        Ok(Self {
            window: Some(Sendable(window)),
            layer,
            format: None,
            pointer_layer: Arc::new(Sendable(pointer_layer)),
            pointer: Arc::default(),
            size,
        })
    }

    /// Has the main thread draw the pointer's news, unless a drawing is already on its way.
    fn draw_pointer(&self, news: impl FnOnce(&mut PointerState)) {
        let Ok(mut state) = self.pointer.lock() else { return };
        news(&mut state);
        if state.pending.is_some() {
            return;
        }
        state.pending = Some(Instant::now());
        drop(state);
        let (layer, pointer, size) = (self.pointer_layer.clone(), self.pointer.clone(), self.size);
        DispatchQueue::main().exec_async(move || {
            if let Ok(mut state) = pointer.lock() {
                draw_pointer(&layer.0, &mut state, size);
            }
        });
    }
}

/// On the main thread: the pointer as `state` has it now.
fn draw_pointer(layer: &CALayer, state: &mut PointerState, size: [u32; 2]) {
    if let Some(came) = state.pending.take() {
        let waited = came.elapsed();
        state.longest = Some(state.longest.map_or(waited, |longest| longest.max(waited)));
    }
    CATransaction::begin();
    // Straight there: a layer would otherwise glide to each new place.
    CATransaction::setDisableActions(true);
    if let Some(shape) = state.shape.take() {
        let image = pointer_image(&shape);
        let contents: Option<&AnyObject> = image.as_ref().map(|image| {
            // SAFETY: a CGImage is an object a layer takes as its contents.
            unsafe { &*CFRetained::as_ptr(image).as_ptr().cast::<AnyObject>() }
        });
        // SAFETY: the layer keeps its own reference to the image.
        unsafe { layer.setContents(contents) };
        state.shape_size = image.is_some().then_some(shape.size);
    }
    let bounds = layer.superlayer().map(|root| root.bounds().size);
    match (state.at, state.shape_size, bounds) {
        (Some(at), Some(shape), Some(bounds)) if at.visible => {
            let [x, y, w, h] = pointer_frame(at.at, shape, size, [bounds.width, bounds.height]);
            layer.setFrame(CGRect::new(CGPoint::new(x, y), CGSize::new(w, h)));
            layer.setHidden(false);
        }
        _ => layer.setHidden(true),
    }
    CATransaction::commit();
}

/// Where the pointer's layer goes, as x, y, width and height in the layer above it (`bounds`
/// points, counting up from the bottom), for a `shape`-sized pointer with its top left `at`
/// on `picture`-sized pictures, which are stretched to fill.
fn pointer_frame(at: [i32; 2], shape: [u32; 2], picture: [u32; 2], bounds: [f64; 2]) -> [f64; 4] {
    let scale = [0, 1].map(|axis| bounds[axis] / f64::from(picture[axis].max(1)));
    let [w, h] = [0, 1].map(|axis| f64::from(shape[axis]) * scale[axis]);
    let x = f64::from(at[0]) * scale[0];
    let y = bounds[1] - f64::from(at[1]) * scale[1] - h;
    [x, y, w, h]
}

/// The pointer's picture, from its straight RGBA.
fn pointer_image(shape: &PointerShape) -> Option<CFRetained<CGImage>> {
    if !shape.sound() {
        return None;
    }
    let [w, h] = shape.size.map(|side| side as usize);
    let data = CFData::from_bytes(&shape.rgba);
    let provider = CGDataProvider::with_cf_data(Some(&data))?;
    // SAFETY: a constant CoreGraphics defines.
    let space = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }))?;
    // SAFETY: the provider holds w * h pixels of four bytes, as `sound` checked.
    unsafe {
        CGImage::new(
            w,
            h,
            8,
            32,
            w * 4,
            Some(&space),
            CGBitmapInfo(CGImageAlphaInfo::Last.0),
            Some(&provider),
            std::ptr::null(),
            true,
            CGColorRenderingIntent::RenderingIntentDefault,
        )
    }
}

impl Showing for Viewer {
    fn picture(&mut self, picture: &Picture) -> Result<(), String> {
        if let Some((sps, pps)) = h264::parameter_sets(&picture.data) {
            self.format = Some(format(sps, pps)?);
        }
        let Some(format) = &self.format else { return Err("No key picture yet".into()) };
        #[allow(deprecated)]
        // SAFETY: the layer is safe to feed from any one thread at a time.
        unsafe {
            // A layer that failed, or stopped decoding (as after the Mac sleeps), starts again
            // from a key picture.
            if self.layer.status() == AVQueuedSampleBufferRenderingStatus::Failed
                || self.layer.requiresFlushToResumeDecoding()
            {
                self.layer.flush();
                if !picture.key {
                    return Err("The picture needs a key picture".into());
                }
            }
            let sample = sample(&h264::to_avcc(&picture.data), format)?;
            self.layer.enqueueSampleBuffer(&sample);
        }
        Ok(())
    }

    fn pointer(&mut self, at: PointerAt) {
        self.draw_pointer(|state| state.at = Some(at));
    }

    fn shape(&mut self, shape: &PointerShape) {
        let shape = shape.clone();
        self.draw_pointer(|state| state.shape = Some(shape));
    }

    fn take_pointer_lag(&mut self) -> Option<Duration> {
        self.pointer.lock().ok().and_then(|mut state| state.longest.take())
    }
}

impl Drop for Viewer {
    fn drop(&mut self) {
        let Some(window) = self.window.take() else { return };
        DispatchQueue::main().exec_async(move || {
            let window = window;
            if let Some(view) = window.0.contentView().and_then(|v| v.downcast::<LentView>().ok()) {
                view.show_cursor();
            }
            window.0.orderOut(None);
            window.0.close();
        });
    }
}

/// A format description from a key picture's parameter sets.
fn format(sps: &[u8], pps: &[u8]) -> Result<CFRetained<CMFormatDescription>, String> {
    let pointers = [NonNull::from(&sps[0]), NonNull::from(&pps[0])];
    let mut sizes = [sps.len(), pps.len()];
    let mut out: *const CMFormatDescription = std::ptr::null();
    // SAFETY: two parameter sets with their sizes; the description is owned from here on.
    unsafe {
        let status = CMVideoFormatDescriptionCreateFromH264ParameterSets(
            None,
            2,
            NonNull::from(&pointers[0]),
            NonNull::from(&mut sizes[0]),
            4,
            NonNull::from(&mut out),
        );
        match NonNull::new(out.cast_mut()) {
            Some(made) if status == 0 => Ok(CFRetained::from_raw(made)),
            _ => Err(format!("The picture's parameters didn't make sense (error {status})")),
        }
    }
}

/// One picture, length-prefixed, as a sample to show at once.
unsafe fn sample(avcc: &[u8], format: &CMFormatDescription) -> Result<CFRetained<CMSampleBuffer>, String> {
    let bytes = avcc.len();
    let source = NonNull::new(avcc.as_ptr().cast_mut()).filter(|_| bytes > 0).ok_or("An empty picture")?;
    // SAFETY: a block CoreMedia allocates and the bytes are copied into; the sample owns it.
    unsafe {
        let mut block: *mut CMBlockBuffer = std::ptr::null_mut();
        let status = CMBlockBuffer::create_with_memory_block(
            None,
            std::ptr::null_mut(),
            bytes,
            None,
            std::ptr::null(),
            0,
            bytes,
            kCMBlockBufferAssureMemoryNowFlag,
            NonNull::from(&mut block),
        );
        let block = NonNull::new(block).filter(|_| status == 0).ok_or("Couldn't hold the picture")?;
        let block = CFRetained::from_raw(block);
        if CMBlockBuffer::replace_data_bytes(source.cast(), &block, 0, bytes) != 0 {
            return Err("Couldn't hold the picture".into());
        }
        let mut sample: *mut CMSampleBuffer = std::ptr::null_mut();
        let sizes = [bytes];
        let status = CMSampleBuffer::create_ready(
            None,
            Some(&block),
            Some(format),
            1,
            0,
            std::ptr::null(),
            1,
            sizes.as_ptr(),
            NonNull::from(&mut sample),
        );
        let sample = NonNull::new(sample).filter(|_| status == 0).ok_or("Couldn't make the picture")?;
        let sample = CFRetained::from_raw(sample);
        if let Some(attachments) = sample.sample_attachments_array(true) {
            let first = attachments.value_at_index(0).cast::<CFMutableDictionary>();
            if let Some(first) = first.as_ref() {
                let key: *const CFString = kCMSampleAttachmentKey_DisplayImmediately;
                let yes: *const CFBoolean = CFBoolean::new(true);
                CFMutableDictionary::set_value(Some(first), key.cast::<c_void>(), yes.cast::<c_void>());
            }
        }
        Ok(sample)
    }
}

/// The window over the screen at `rect` (in points, top left at the top), with the layer
/// that shows the pictures and the one above it for the host's pointer.
fn make_window(
    mtm: MainThreadMarker,
    rect: Rect,
    remote: RemoteSink,
) -> (Retained<LentWindow>, Retained<AVSampleBufferDisplayLayer>, Retained<CALayer>) {
    // AppKit counts up from the bottom of the first screen.
    let first = NSScreen::screens(mtm).firstObject().map_or(rect.h, |screen| screen.frame().size.height);
    let frame = NSRect::new(NSPoint::new(rect.x, first - rect.y - rect.h), NSSize::new(rect.w, rect.h));
    let window: Retained<LentWindow> = {
        let allocated = LentWindow::alloc(mtm);
        // SAFETY: NSWindow's own initializer; the subclass adds no state.
        unsafe {
            msg_send![
                allocated,
                initWithContentRect: frame,
                styleMask: NSWindowStyleMask::Borderless,
                backing: NSBackingStoreType::Buffered,
                defer: false,
            ]
        }
    };
    // SAFETY: plain window settings on the main thread.
    unsafe { window.setReleasedWhenClosed(false) };
    window.setLevel(LEVEL);
    window.setOpaque(true);
    window.setHasShadow(false);
    window.setBackgroundColor(Some(&NSColor::blackColor()));
    window.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::Stationary
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::IgnoresCycle,
    );
    window.setAcceptsMouseMovedEvents(true);
    // SAFETY: a layer made and set up before anything else can see it.
    let layer = unsafe {
        let layer = AVSampleBufferDisplayLayer::new();
        if let Some(resize) = AVLayerVideoGravityResize {
            layer.setVideoGravity(resize);
        }
        layer
    };
    // The pictures and the pointer side by side under a plain layer, the pointer on top.
    let (root, pointer) = (CALayer::layer(), CALayer::layer());
    let bounds = CGRect::new(CGPoint::ZERO, CGSize::new(rect.w, rect.h));
    root.setFrame(bounds);
    layer.setFrame(bounds);
    layer.setAutoresizingMask(CAAutoresizingMask::LayerWidthSizable | CAAutoresizingMask::LayerHeightSizable);
    root.addSublayer(&layer);
    pointer.setHidden(true);
    pointer.setZPosition(1.0);
    root.addSublayer(&pointer);
    let view = LentView::new(mtm, remote);
    view.setLayer(Some(&root));
    view.setWantsLayer(true);
    // SAFETY: a tracking area the view owns, over all of it.
    unsafe {
        let options = NSTrackingAreaOptions::MouseEnteredAndExited
            | NSTrackingAreaOptions::MouseMoved
            | NSTrackingAreaOptions::ActiveAlways
            | NSTrackingAreaOptions::InVisibleRect;
        let owner: &AnyObject = &view;
        let area = NSTrackingArea::initWithRect_options_owner_userInfo(
            NSTrackingArea::alloc(),
            NSRect::ZERO,
            options,
            Some(owner),
            None,
        );
        view.addTrackingArea(&area);
    }
    window.setContentView(Some(&view));
    window.orderFrontRegardless();
    window.makeFirstResponder(Some(&view));
    (window, layer, pointer)
}

define_class!(
    /// A borderless window that can still take the keyboard.
    #[unsafe(super(NSWindow, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "RoamLentWindow"]
    struct LentWindow;

    unsafe impl NSObjectProtocol for LentWindow {}

    impl LentWindow {
        #[unsafe(method(canBecomeKeyWindow))]
        fn can_become_key(&self) -> bool {
            true
        }

        #[unsafe(method(canBecomeMainWindow))]
        fn can_become_main(&self) -> bool {
            true
        }
    }
);

struct ViewState {
    remote: RemoteSink,
    /// Modifier keys down, which macOS only reports as changes.
    modifiers: RefCell<HashSet<u16>>,
    hidden: Cell<bool>,
}

define_class!(
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "RoamLentView"]
    #[ivars = ViewState]
    struct LentView;

    unsafe impl NSObjectProtocol for LentView {}

    impl LentView {
        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool {
            true
        }

        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }

        #[unsafe(method(mouseEntered:))]
        fn mouse_entered(&self, _event: &NSEvent) {
            self.hide_cursor();
            if let Some(window) = self.window() {
                #[allow(deprecated)]
                NSApplication::sharedApplication(self.mtm()).activateIgnoringOtherApps(true);
                window.makeKeyWindow();
                window.makeFirstResponder(Some(self));
            }
        }

        #[unsafe(method(mouseExited:))]
        fn mouse_exited(&self, _event: &NSEvent) {
            self.show_cursor();
        }

        #[unsafe(method(mouseMoved:))]
        fn mouse_moved(&self, event: &NSEvent) {
            self.moved(event);
        }

        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, event: &NSEvent) {
            self.moved(event);
        }

        #[unsafe(method(rightMouseDragged:))]
        fn right_mouse_dragged(&self, event: &NSEvent) {
            self.moved(event);
        }

        #[unsafe(method(otherMouseDragged:))]
        fn other_mouse_dragged(&self, event: &NSEvent) {
            self.moved(event);
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            self.moved(event);
            self.send(Remote::Button { button: 0, down: true });
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, _event: &NSEvent) {
            self.send(Remote::Button { button: 0, down: false });
        }

        #[unsafe(method(rightMouseDown:))]
        fn right_mouse_down(&self, event: &NSEvent) {
            self.moved(event);
            self.send(Remote::Button { button: 1, down: true });
        }

        #[unsafe(method(rightMouseUp:))]
        fn right_mouse_up(&self, _event: &NSEvent) {
            self.send(Remote::Button { button: 1, down: false });
        }

        #[unsafe(method(otherMouseDown:))]
        fn other_mouse_down(&self, event: &NSEvent) {
            self.moved(event);
            self.send(Remote::Button { button: other_button(event), down: true });
        }

        #[unsafe(method(otherMouseUp:))]
        fn other_mouse_up(&self, event: &NSEvent) {
            self.send(Remote::Button { button: other_button(event), down: false });
        }

        #[unsafe(method(scrollWheel:))]
        fn scroll_wheel(&self, event: &NSEvent) {
            let (dx, dy) = if event.hasPreciseScrollingDeltas() {
                let scale = 120.0 / PIXELS_PER_NOTCH;
                (-event.scrollingDeltaX() * scale, event.scrollingDeltaY() * scale)
            } else {
                (-event.scrollingDeltaX() * 120.0, event.scrollingDeltaY() * 120.0)
            };
            if dx != 0.0 || dy != 0.0 {
                self.send(Remote::Wheel { dx: dx as f32, dy: dy as f32 });
            }
        }

        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            if !event.isARepeat() {
                self.key(event, true);
            }
        }

        #[unsafe(method(keyUp:))]
        fn key_up(&self, event: &NSEvent) {
            self.key(event, false);
        }

        #[unsafe(method(flagsChanged:))]
        fn flags_changed(&self, event: &NSEvent) {
            let code = event.keyCode();
            let Some(hid) = keys::from_mac(code) else { return };
            let down = if code == CAPS_LOCK {
                event.modifierFlags().contains(NSEventModifierFlags::CapsLock)
            } else {
                let mut held = self.ivars().modifiers.borrow_mut();
                if held.remove(&hid) { false } else { held.insert(hid) }
            };
            self.send(Remote::Key { hid, down });
            if code == CAPS_LOCK && down {
                // The host toggles on the press; let it go at once.
                self.send(Remote::Key { hid, down: false });
            }
        }

        // Keys go to the host only, never to a menu or a beep here.
        #[unsafe(method(performKeyEquivalent:))]
        fn perform_key_equivalent(&self, event: &NSEvent) -> bool {
            let mine = self.window().is_some_and(|w| w.isKeyWindow());
            if mine && !event.isARepeat() {
                self.key(event, true);
            }
            mine
        }
    }
);

/// macOS's key code for caps lock, which reports its state rather than presses.
const CAPS_LOCK: u16 = 57;

fn other_button(event: &NSEvent) -> u8 {
    match event.buttonNumber() {
        2 => 2,
        n => n.clamp(3, 4) as u8,
    }
}

impl LentView {
    fn new(mtm: MainThreadMarker, remote: RemoteSink) -> Retained<Self> {
        let state = ViewState { remote, modifiers: RefCell::default(), hidden: Cell::new(false) };
        let this = Self::alloc(mtm).set_ivars(state);
        // SAFETY: NSView's own initializer.
        unsafe { msg_send![super(this), init] }
    }

    fn send(&self, remote: Remote) {
        (self.ivars().remote)(remote);
    }

    fn moved(&self, event: &NSEvent) {
        let at = self.convertPoint_fromView(event.locationInWindow(), None);
        let bounds = self.bounds();
        let (w, h) = (bounds.size.width.max(1.0), bounds.size.height.max(1.0));
        // Views count up from the bottom.
        let (x, y) = (at.x / w, (h - at.y) / h);
        // Once this window is key, macOS sends it every move of the mouse, wherever it is: on
        // this computer's other screens too, where Roam may be moving it for the host. Those
        // aren't over the lent screen and would pull the host's cursor back onto it.
        if !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) {
            return;
        }
        self.send(Remote::Move([x as f32, y as f32]));
    }

    fn key(&self, event: &NSEvent, down: bool) {
        if let Some(hid) = keys::from_mac(event.keyCode()) {
            self.send(Remote::Key { hid, down });
        }
    }

    /// The host's pointer is in the picture, or drawn over it.
    fn hide_cursor(&self) {
        if self.ivars().hidden.replace(true) {
            return;
        }
        // SAFETY: plain CoreGraphics calls, kept balanced by `hidden`.
        unsafe {
            let key = CFString::from_static_str("SetsCursorInBackground");
            let connection = _CGSDefaultConnection();
            let key: *const CFString = &*key;
            let yes: *const CFBoolean = CFBoolean::new(true);
            CGSSetConnectionProperty(connection, connection, key.cast_mut().cast(), yes.cast_mut().cast());
            CGDisplayHideCursor(CGMainDisplayID());
        }
    }

    fn show_cursor(&self) {
        if self.ivars().hidden.replace(false) {
            // SAFETY: balances the hide above.
            unsafe { CGDisplayShowCursor(CGMainDisplayID()) };
        }
    }
}

#[cfg(test)]
mod tests {
    use objc2_core_graphics::{CGContext, CGImageByteOrderInfo};

    use super::*;

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGBitmapContextCreate(
            data: *mut c_void,
            width: usize,
            height: usize,
            bits_per_component: usize,
            bytes_per_row: usize,
            space: *const CGColorSpace,
            bitmap_info: u32,
        ) -> *mut CGContext;
        fn CGContextRelease(context: *mut CGContext);
    }

    #[test]
    fn the_pointer_lands_where_the_host_has_it() {
        // A 1920x1080 picture stretched over a 960x600 screen: half across, 5/9 down.
        let [x, y, w, h] = pointer_frame([960, 540], [32, 36], [1920, 1080], [960.0, 600.0]);
        assert_eq!([x, w], [480.0, 16.0]);
        assert!((h - 20.0).abs() < 1e-9 && (y - (600.0 - 300.0 - 20.0)).abs() < 1e-9, "{y} {h}");
        assert_eq!(pointer_frame([0, 0], [10, 10], [100, 100], [100.0, 100.0]), [0.0, 90.0, 10.0, 10.0], "top left");
    }

    /// Drawn for real, off screen: the pointer's top left pixel lands at the top left of where
    /// the host put it, the right way up.
    #[test]
    fn the_pointer_draws_upright_at_its_place() {
        let (width, height) = (100usize, 80usize);
        let root = CALayer::layer();
        root.setFrame(CGRect::new(CGPoint::ZERO, CGSize::new(width as f64, height as f64)));
        let layer = CALayer::layer();
        root.addSublayer(&layer);
        // Red at the top left, blue everywhere else, on a 4x4 pointer.
        let mut rgba = [0u8, 0, 255, 255].repeat(16);
        rgba[..4].copy_from_slice(&[255, 0, 0, 255]);
        let mut state = PointerState {
            at: Some(PointerAt { at: [20, 10], visible: true }),
            shape: Some(PointerShape { size: [4, 4], rgba }),
            ..PointerState::default()
        };
        draw_pointer(&layer, &mut state, [width as u32, height as u32]);
        assert!(!layer.isHidden());
        let mut pixels = vec![0u8; width * height * 4];
        let space = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB })).unwrap();
        let info = CGImageAlphaInfo::PremultipliedLast.0 | CGImageByteOrderInfo::Order32Big.0;
        // SAFETY: a context over `pixels`, released before they go.
        unsafe {
            let context = CGBitmapContextCreate(pixels.as_mut_ptr().cast(), width, height, 8, width * 4, &*space, info);
            assert!(!context.is_null());
            root.renderInContext(&*context);
            CGContextRelease(context);
        }
        // The context's first row is its top.
        let pixel = |x: usize, y: usize| <[u8; 4]>::try_from(&pixels[(y * width + x) * 4..][..4]).unwrap();
        assert_eq!(pixel(20, 10), [255, 0, 0, 255], "the top left pixel, at the top left");
        assert_eq!(pixel(23, 13), [0, 0, 255, 255]);
        assert_eq!(pixel(21, 10)[2], 255, "blue beside it");
        assert_eq!(pixel(19, 10)[3], 0, "nothing outside");
        assert_eq!(pixel(20, 14)[3], 0, "nothing below");
        state.at = Some(PointerAt { at: [20, 10], visible: false });
        draw_pointer(&layer, &mut state, [width as u32, height as u32]);
        assert!(layer.isHidden());
    }
}
