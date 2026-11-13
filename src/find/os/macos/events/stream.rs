//! The FSEvents stream itself, on a dispatch queue, handing what it hears to the writer.

use std::ffi::{CStr, c_char, c_void};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::ptr;
use std::sync::Arc;
use std::sync::mpsc::Sender;

use objc2::rc::Retained;
use objc2_foundation::{NSArray, NSString};

use crate::find::os::Root;
use crate::find::shared::Order;
use crate::find::watch::flags::{self, Fired};
use crate::find::watch::{Burst, Change, Event};

use super::super::ffi::{DispatchQueue, dispatch_queue_create, dispatch_release};

type StreamRef = *mut c_void;
type Callback = extern "C" fn(*const c_void, *mut c_void, usize, *mut c_void, *const u32, *const u64);

const SINCE_NOW: u64 = u64::MAX;
const LATENCY: f64 = 0.1;
const NO_DEFER: u32 = 0x02;
const WATCH_ROOT: u32 = 0x04;
const FILE_EVENTS: u32 = 0x10;
const FLAGS: u32 = FILE_EVENTS | NO_DEFER | WATCH_ROOT;
const QUEUE_LABEL: &CStr = c"notchbase-find-watch";

#[repr(C)]
struct Context {
    version: isize,
    info: *mut c_void,
    retain: Option<extern "C" fn(*const c_void) -> *const c_void>,
    release: Option<extern "C" fn(*const c_void)>,
    describe: *const c_void,
}

#[link(name = "CoreServices", kind = "framework")]
unsafe extern "C" {
    fn FSEventStreamCreate(
        allocator: *const c_void,
        callback: Callback,
        context: *const Context,
        paths: *const c_void,
        since: u64,
        latency: f64,
        flags: u32,
    ) -> StreamRef;
    fn FSEventStreamSetDispatchQueue(stream: StreamRef, queue: DispatchQueue);
    fn FSEventStreamStart(stream: StreamRef) -> u8;
    fn FSEventStreamStop(stream: StreamRef);
    fn FSEventStreamInvalidate(stream: StreamRef);
    fn FSEventStreamRelease(stream: StreamRef);
    pub fn FSEventsGetCurrentEventId() -> u64;
}

unsafe extern "C" {
    fn dispatch_sync_f(queue: DispatchQueue, context: *mut c_void, work: extern "C" fn(*mut c_void));
}

struct Sink {
    orders: Sender<Order>,
    roots: Vec<PathBuf>,
}

pub(super) struct Stream {
    stream: StreamRef,
    queue: DispatchQueue,
    _sink: Arc<Sink>,
}

unsafe impl Send for Stream {}

impl Stream {
    pub(super) fn start(roots: &[Root], since: Option<u64>, orders: Sender<Order>) -> Option<Self> {
        let watched: Vec<Retained<NSString>> =
            roots.iter().map(|found| NSString::from_str(&found.path.to_string_lossy())).collect();
        let paths = NSArray::from_retained_slice(&watched);
        let sink = Arc::new(Sink { orders, roots: roots.iter().map(|found| found.path.clone()).collect() });
        let info = Arc::as_ptr(&sink).cast_mut().cast::<c_void>();
        let context = Context { version: 0, info, retain: Some(retain), release: Some(release), describe: ptr::null() };
        let since = since.unwrap_or(SINCE_NOW);
        let list = Retained::as_ptr(&paths).cast::<c_void>();
        let stream = unsafe { FSEventStreamCreate(ptr::null(), fired, &context, list, since, LATENCY, FLAGS) };
        if stream.is_null() {
            return None;
        }
        let queue = unsafe { dispatch_queue_create(QUEUE_LABEL.as_ptr(), ptr::null()) };
        unsafe { FSEventStreamSetDispatchQueue(stream, queue) };
        if unsafe { FSEventStreamStart(stream) } == 0 {
            unsafe { discard(stream, queue) };
            return None;
        }
        Some(Self { stream, queue, _sink: sink })
    }
}

unsafe fn discard(stream: StreamRef, queue: DispatchQueue) {
    unsafe {
        FSEventStreamInvalidate(stream);
        FSEventStreamRelease(stream);
        dispatch_release(queue);
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        unsafe {
            FSEventStreamStop(self.stream);
            FSEventStreamInvalidate(self.stream);
            dispatch_sync_f(self.queue, ptr::null_mut(), drained);
            FSEventStreamRelease(self.stream);
            dispatch_release(self.queue);
        }
    }
}

extern "C" fn drained(_context: *mut c_void) {}

extern "C" fn retain(info: *const c_void) -> *const c_void {
    unsafe { Arc::increment_strong_count(info.cast::<Sink>()) };
    info
}

extern "C" fn release(info: *const c_void) {
    unsafe { Arc::decrement_strong_count(info.cast::<Sink>()) };
}

extern "C" fn fired(
    _: *const c_void,
    info: *mut c_void,
    count: usize,
    paths: *mut c_void,
    bits: *const u32,
    ids: *const u64,
) {
    if info.is_null() || count == 0 || paths.is_null() || bits.is_null() || ids.is_null() {
        return;
    }
    let sink = unsafe { &*info.cast::<Sink>() };
    let read = catch_unwind(AssertUnwindSafe(|| {
        let paths = unsafe { std::slice::from_raw_parts(paths.cast::<*const c_char>(), count) };
        let (bits, ids) = unsafe { (std::slice::from_raw_parts(bits, count), std::slice::from_raw_parts(ids, count)) };
        let fired: Vec<Fired> = paths
            .iter()
            .zip(bits.iter().zip(ids))
            .filter(|(path, _)| !path.is_null())
            .map(|(path, (bits, id))| Fired::new(&unsafe { CStr::from_ptr(*path) }.to_string_lossy(), *bits, *id))
            .collect();
        flags::burst(&fired, &sink.roots)
    }));
    let burst = read.unwrap_or_else(|_| {
        let id = unsafe { *ids };
        let events = sink.roots.iter().map(|root| Event { change: Change::Overflow(root.clone()), id }).collect();
        Burst { events, caught_up: false }
    });
    if !burst.events.is_empty() || burst.caught_up {
        let _ = sink.orders.send(Order::Changes(burst));
    }
}
