//! Drops on macOS. winit makes the window delegate the drag destination, so its four dragging methods are replaced
//! once with ours, and file promises are received into Downloads.

use std::ffi::CString;
use std::path::PathBuf;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, Once, PoisonError};

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Bool, Imp, ProtocolObject, Sel};
use objc2::{Encode, ffi, sel};
use objc2_app_kit::{NSDragOperation, NSDraggingInfo, NSFilePromiseReceiver, NSWindow};
use objc2_foundation::{NSDictionary, NSError, NSOperationQueue, NSString, NSURL};
use winit::window::{Window, WindowId};

use super::super::incoming::{Arrival, Inbox, Saved};
use super::super::save::rescue;
use super::drag::content_view;
use super::pasteboard;

const NO_DOWNLOADS: &str = "Couldn't find your Downloads folder";
const PROMISE_BROKEN: &str = "The app you dragged from couldn't save it";

type Deciding = extern "C-unwind" fn(&AnyObject, Sel, &ProtocolObject<dyn NSDraggingInfo>) -> NSDragOperation;
type Leaving = extern "C-unwind" fn(&AnyObject, Sel, Option<&ProtocolObject<dyn NSDraggingInfo>>);
type Taking = extern "C-unwind" fn(&AnyObject, Sel, &ProtocolObject<dyn NSDraggingInfo>) -> Bool;

static SITES: Mutex<Vec<(usize, WindowId, Inbox)>> = Mutex::new(Vec::new());
static TAKEN_OVER: Once = Once::new();
static ACCEPTING: AtomicBool = AtomicBool::new(false);
static CAUGHT: AtomicBool = AtomicBool::new(false);

pub(in crate::filedrag) struct Site {
    window: usize,
}

impl Drop for Site {
    fn drop(&mut self) {
        sites().retain(|(window, _, _)| *window != self.window);
    }
}

pub(in crate::filedrag) fn accept_drops(window: &Window, inbox: Inbox) -> Option<Site> {
    let ns_window = content_view(window).ok()?.window()?;
    let delegate = ns_window.delegate()?;
    let delegate: &AnyObject = delegate.as_ref();
    let class = delegate.class();
    TAKEN_OVER.call_once(|| take_over(class));
    ns_window.registerForDraggedTypes(&pasteboard::accepted_types());
    let key = key(&ns_window);
    sites().push((key, window.id(), inbox));
    Some(Site { window: key })
}

/// Swaps our dragging methods into the delegate's class with class_replaceMethod, once for every window.
fn take_over(class: &AnyClass) {
    let class = std::ptr::from_ref(class).cast_mut();
    let deciding = encoding(&NSDragOperation::ENCODING.to_string());
    let leaving = encoding("v");
    let taking = encoding(&Bool::ENCODING.to_string());
    let methods: [(Sel, Imp, &CString); 4] = unsafe {
        [
            (sel!(draggingEntered:), std::mem::transmute::<Deciding, Imp>(entered), deciding),
            (sel!(draggingUpdated:), std::mem::transmute::<Deciding, Imp>(updated), deciding),
            (sel!(draggingExited:), std::mem::transmute::<Leaving, Imp>(exited), leaving),
            (sel!(performDragOperation:), std::mem::transmute::<Taking, Imp>(perform), taking),
        ]
    };
    for (selector, implementation, types) in methods {
        unsafe { ffi::class_replaceMethod(class, selector, implementation, types.as_ptr()) };
    }
}

fn encoding(returns: &str) -> &'static CString {
    let types = CString::new(format!("{returns}@:@")).unwrap_or_default();
    Box::leak(Box::new(types))
}

extern "C-unwind" fn entered(_: &AnyObject, _: Sel, info: &ProtocolObject<dyn NSDraggingInfo>) -> NSDragOperation {
    let Some((window, inbox)) = site(info) else {
        return NSDragOperation::None;
    };
    let count = pasteboard::count(&info.draggingPasteboard());
    ACCEPTING.store(count > 0, Ordering::Relaxed);
    if count == 0 {
        return catch();
    }
    inbox.post(window, Arrival::Hover(count));
    if let Some(page) = pasteboard::offered_page(&info.draggingPasteboard()) {
        inbox.post(window, Arrival::Offer(page));
    }
    NSDragOperation::Copy
}

extern "C-unwind" fn updated(_: &AnyObject, _: Sel, _: &ProtocolObject<dyn NSDraggingInfo>) -> NSDragOperation {
    if ACCEPTING.load(Ordering::Relaxed) { NSDragOperation::Copy } else { catch() }
}

/// A drag we can't read is still taken while catching everything, so the tab stays where it was.
fn catch() -> NSDragOperation {
    let caught = super::super::catches_everything();
    CAUGHT.store(caught, Ordering::Relaxed);
    if caught { NSDragOperation::Copy } else { NSDragOperation::None }
}

extern "C-unwind" fn exited(_: &AnyObject, _: Sel, info: Option<&ProtocolObject<dyn NSDraggingInfo>>) {
    CAUGHT.store(false, Ordering::Relaxed);
    let accepted = ACCEPTING.swap(false, Ordering::Relaxed);
    if let Some((window, inbox)) = info.and_then(site)
        && accepted
    {
        inbox.post(window, Arrival::Leave);
    }
}

extern "C-unwind" fn perform(_: &AnyObject, _: Sel, info: &ProtocolObject<dyn NSDraggingInfo>) -> Bool {
    ACCEPTING.store(false, Ordering::Relaxed);
    let caught = CAUGHT.swap(false, Ordering::Relaxed);
    let Some((window, inbox)) = site(info) else {
        return Bool::NO;
    };
    let found = pasteboard::read(&info.draggingPasteboard());
    let taken = caught || !found.dropped.is_empty() || !found.promises.is_empty();
    receive(found.promises, window, &inbox);
    let dropped = rescue(found.dropped, inbox.folder());
    let arrival = if dropped.is_empty() { Arrival::Leave } else { Arrival::Drop(dropped) };
    inbox.post(window, arrival);
    Bool::new(taken)
}

/// Asks each app that promised a file to write it into Downloads. Each file posts `Saved` when it lands.
fn receive(promises: Vec<Retained<NSFilePromiseReceiver>>, window: WindowId, inbox: &Inbox) {
    if promises.is_empty() {
        return;
    }
    let Some(folder) = inbox.folder() else {
        inbox.post(window, Arrival::Saved(Saved { paths: Vec::new(), problems: vec![NO_DOWNLOADS.into()] }));
        return;
    };
    let _ = std::fs::create_dir_all(folder);
    let destination = NSURL::fileURLWithPath_isDirectory(&NSString::from_str(&folder.to_string_lossy()), true);
    let queue = NSOperationQueue::new();
    let options = NSDictionary::new();
    for promise in promises {
        let inbox = inbox.clone();
        let reader = RcBlock::new(move |file: NonNull<NSURL>, error: *mut NSError| {
            let path = unsafe { file.as_ref() }.path().map(|path| PathBuf::from(path.to_string()));
            let saved = match path {
                Some(path) if error.is_null() => Saved { paths: vec![path], problems: Vec::new() },
                _ => Saved { paths: Vec::new(), problems: vec![PROMISE_BROKEN.into()] },
            };
            inbox.post(window, Arrival::Saved(saved));
        });
        unsafe {
            promise.receivePromisedFilesAtDestination_options_operationQueue_reader(
                &destination,
                &options,
                &queue,
                &reader,
            );
        }
    }
}

fn site(info: &ProtocolObject<dyn NSDraggingInfo>) -> Option<(WindowId, Inbox)> {
    let window = info.draggingDestinationWindow()?;
    let key = key(&window);
    sites().iter().find(|(window, _, _)| *window == key).map(|(_, id, inbox)| (*id, inbox.clone()))
}

/// Sites are found by their NSWindow's address.
fn key(window: &NSWindow) -> usize {
    std::ptr::from_ref(window) as usize
}

fn sites() -> std::sync::MutexGuard<'static, Vec<(usize, WindowId, Inbox)>> {
    SITES.lock().unwrap_or_else(PoisonError::into_inner)
}
