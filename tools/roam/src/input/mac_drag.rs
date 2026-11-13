//! Dragging files across the edge of the screen on a Mac. A drag from Finder or any other app
//! puts its files on the system's drag pasteboard, so a drag that started since the left button
//! went down can be read from there. Files coming the other way are picked up from a tiny,
//! almost invisible window put under the cursor: the left button goes down on it, it starts a
//! drag of the files as Finder would, and letting the button go drops them.

use std::cell::RefCell;
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{AllocAnyThread as _, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSBackingStoreType, NSColor, NSDragOperation, NSDraggingContext, NSDraggingItem, NSDraggingSession,
    NSDraggingSource, NSEvent, NSPasteboard, NSPasteboardNameDrag, NSPasteboardTypeFileURL, NSResponder, NSView,
    NSWindow, NSWindowCollectionBehavior, NSWindowStyleMask, NSWorkspace,
};
use objc2_foundation::{NSArray, NSPoint, NSRect, NSSize, NSString, NSURL};

const SIZE: f64 = 32.0;
const ICON: f64 = 64.0;
/// Above every other window, even a full-screen app's.
const LEVEL: isize = 1000;

/// Files waiting to be picked up when the button goes down on the window.
static PENDING: Mutex<Option<Vec<PathBuf>>> = Mutex::new(None);

thread_local! {
    static WINDOW: RefCell<Option<Retained<NSWindow>>> = const { RefCell::new(None) };
}

/// The drag pasteboard's count of changes, which goes up when a drag starts.
pub fn drag_count() -> isize {
    autoreleasepool(|_| drag_pasteboard().changeCount())
}

/// The files of the drag under way, if one started since the count was `since`.
pub fn dragged_files(since: isize) -> Option<Vec<PathBuf>> {
    autoreleasepool(|_| {
        let pasteboard = drag_pasteboard();
        if pasteboard.changeCount() == since {
            return None;
        }
        let kind = unsafe { NSPasteboardTypeFileURL };
        let paths: Vec<PathBuf> = pasteboard
            .pasteboardItems()?
            .iter()
            .filter_map(|item| item.stringForType(kind))
            .filter_map(|link| NSURL::URLWithString(&link))
            .filter_map(|url| url.path())
            .map(|path| PathBuf::from(path.to_string()))
            .collect();
        (!paths.is_empty()).then_some(paths)
    })
}

fn drag_pasteboard() -> Retained<NSPasteboard> {
    NSPasteboard::pasteboardWithName(unsafe { NSPasteboardNameDrag })
}

/// Picks `paths` up under the cursor: puts the window there, then `press` presses the left
/// button on it once it's up.
pub fn drag_in(paths: Vec<PathBuf>, press: impl FnOnce() + Send + 'static) {
    *PENDING.lock().unwrap_or_else(|e| e.into_inner()) = Some(paths);
    on_main(move || {
        let Some(mtm) = MainThreadMarker::new() else { return };
        let window = WINDOW.with_borrow_mut(|window| window.get_or_insert_with(|| make_window(mtm)).clone());
        let at = NSEvent::mouseLocation();
        let frame = NSRect::new(NSPoint::new(at.x - SIZE / 2.0, at.y - SIZE / 2.0), NSSize::new(SIZE, SIZE));
        window.setFrame_display(frame, false);
        window.setAlphaValue(1.0);
        window.setIgnoresMouseEvents(false);
        window.orderFrontRegardless();
        // The window server needs a moment to put the window where clicks find it.
        let _ = std::thread::Builder::new().name("roam-pick".into()).spawn(move || {
            std::thread::sleep(Duration::from_millis(60));
            press();
        });
    });
}

fn make_window(mtm: MainThreadMarker) -> Retained<NSWindow> {
    let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(SIZE, SIZE));
    // SAFETY: a plain borderless window, kept for as long as the program runs.
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            frame,
            NSWindowStyleMask::Borderless,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    // SAFETY: the window is kept in WINDOW, so closing it mustn't free it.
    unsafe { window.setReleasedWhenClosed(false) };
    window.setOpaque(false);
    window.setHasShadow(false);
    // Nearly clear, but still there for a click to land on.
    window.setBackgroundColor(Some(&NSColor::colorWithWhite_alpha(0.0, 0.01)));
    window.setLevel(LEVEL);
    window.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::Stationary
            | NSWindowCollectionBehavior::IgnoresCycle,
    );
    let view = DragView::new(mtm, frame);
    window.setContentView(Some(&view));
    window
}

fn hide(window: &NSWindow) {
    window.setIgnoresMouseEvents(true);
    window.setAlphaValue(0.0);
    window.orderOut(None);
}

define_class!(
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "RoamDragView"]
    struct DragView;

    unsafe impl NSObjectProtocol for DragView {}

    impl DragView {
        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            let pending = PENDING.lock().unwrap_or_else(|e| e.into_inner()).take();
            let Some(paths) = pending else {
                if let Some(window) = self.window() {
                    hide(&window);
                }
                return;
            };
            let at = self.convertPoint_fromView(event.locationInWindow(), None);
            let frame = NSRect::new(NSPoint::new(at.x - ICON / 2.0, at.y - ICON / 2.0), NSSize::new(ICON, ICON));
            let items = dragging_items(&paths, frame);
            if items.is_empty() {
                if let Some(window) = self.window() {
                    hide(&window);
                }
                return;
            }
            let session = self.beginDraggingSessionWithItems_event_source(
                &NSArray::from_retained_slice(&items),
                event,
                ProtocolObject::from_ref(self),
            );
            session.setAnimatesToStartingPositionsOnCancelOrFail(true);
            // Out of the way of whatever the files are dropped on.
            if let Some(window) = self.window() {
                window.setIgnoresMouseEvents(true);
                window.setAlphaValue(0.0);
            }
        }
    }

    unsafe impl NSDraggingSource for DragView {
        #[unsafe(method(draggingSession:sourceOperationMaskForDraggingContext:))]
        fn operations(&self, _session: &NSDraggingSession, _context: NSDraggingContext) -> NSDragOperation {
            NSDragOperation::Copy
        }

        #[unsafe(method(draggingSession:endedAtPoint:operation:))]
        fn ended(&self, _session: &NSDraggingSession, _point: NSPoint, _operation: NSDragOperation) {
            if let Some(window) = self.window() {
                hide(&window);
            }
        }
    }
);

impl DragView {
    fn new(mtm: MainThreadMarker, frame: NSRect) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(());
        // SAFETY: NSView's designated initialiser.
        unsafe { msg_send![super(this), initWithFrame: frame] }
    }
}

fn dragging_items(paths: &[PathBuf], frame: NSRect) -> Vec<Retained<NSDraggingItem>> {
    let workspace = NSWorkspace::sharedWorkspace();
    paths.iter().filter_map(|path| dragging_item(&workspace, path, frame)).collect()
}

fn dragging_item(workspace: &NSWorkspace, path: &Path, frame: NSRect) -> Option<Retained<NSDraggingItem>> {
    let path = NSString::from_str(path.to_str()?);
    let url = NSURL::fileURLWithPath(&path);
    let item = NSDraggingItem::initWithPasteboardWriter(NSDraggingItem::alloc(), ProtocolObject::from_ref(&*url));
    let icon = workspace.iconForFile(&path);
    icon.setSize(frame.size);
    let contents: &AnyObject = &icon;
    // SAFETY: an NSImage is what the frame's contents may be.
    unsafe { item.setDraggingFrame_contents(frame, Some(contents)) };
    Some(item)
}

unsafe extern "C" {
    static _dispatch_main_q: c_void;
    fn dispatch_async_f(queue: *const c_void, context: *mut c_void, work: extern "C" fn(*mut c_void));
}

/// Runs `work` on the main thread, which AppKit's windows and drags need.
fn on_main(work: impl FnOnce() + Send + 'static) {
    extern "C" fn run(context: *mut c_void) {
        // SAFETY: `context` is the box made below, run once.
        let work = unsafe { Box::from_raw(context.cast::<Box<dyn FnOnce()>>()) };
        work();
    }
    let work: Box<Box<dyn FnOnce()>> = Box::new(Box::new(work));
    // SAFETY: the main queue lives for the whole program, and `run` takes the box back.
    unsafe { dispatch_async_f(&raw const _dispatch_main_q, Box::into_raw(work).cast(), run) };
}
