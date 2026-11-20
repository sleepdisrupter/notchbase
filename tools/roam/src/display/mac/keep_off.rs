//! Keeps this Mac's own windows off a screen it lends: the host's pictures cover that screen,
//! so a window left there, or opened there later, would be out of sight behind them.
//!
//! While the screen is lent, a thread looks about twice a second for ordinary windows on it
//! (from the window server's list, which needs no permission) and moves each one, through the
//! accessibility API Roam already has permission for, to the Mac's biggest other screen, made
//! smaller if it wouldn't fit there.

use std::ffi::c_void;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use objc2_core_foundation::CFString;

use crate::layout::Rect;

/// How often to look.
const EVERY: Duration = Duration::from_millis(500);
/// How far in from the other screen's corner a moved window lands, in points.
const INSET: f64 = 40.0;
/// How long an app may take to answer before it's left for the next look.
const PATIENCE: f32 = 0.25;

type Ref = *mut c_void;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Point {
    x: f64,
    y: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Size {
    width: f64,
    height: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Bounds {
    origin: Point,
    size: Size,
}

const AX_VALUE_POINT: u32 = 1;
const AX_VALUE_SIZE: u32 = 2;
const ON_SCREEN_ONLY: u32 = 1;
const EXCLUDE_DESKTOP: u32 = 16;
const NUMBER_SINT32: isize = 3;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXUIElementCreateApplication(pid: i32) -> Ref;
    fn AXUIElementCopyAttributeValue(element: Ref, attribute: Ref, value: *mut Ref) -> i32;
    fn AXUIElementSetAttributeValue(element: Ref, attribute: Ref, value: Ref) -> i32;
    fn AXUIElementSetMessagingTimeout(element: Ref, seconds: f32) -> i32;
    fn AXValueCreate(kind: u32, value: *const c_void) -> Ref;
    fn AXValueGetValue(value: Ref, kind: u32, out: *mut c_void) -> u8;
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGWindowListCopyWindowInfo(option: u32, relative_to: u32) -> Ref;
    fn CGRectMakeWithDictionaryRepresentation(dictionary: Ref, rect: *mut Bounds) -> u8;
    fn CGGetActiveDisplayList(most: u32, displays: *mut u32, count: *mut u32) -> i32;
    fn CGDisplayBounds(display: u32) -> Bounds;
    static kCGWindowOwnerPID: Ref;
    static kCGWindowLayer: Ref;
    static kCGWindowBounds: Ref;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFArrayGetCount(array: Ref) -> isize;
    fn CFArrayGetValueAtIndex(array: Ref, index: isize) -> Ref;
    fn CFDictionaryGetValue(dictionary: Ref, key: Ref) -> Ref;
    fn CFNumberGetValue(number: Ref, kind: isize, value: *mut c_void) -> u8;
    fn CFRelease(object: Ref);
}

/// Keeps windows off `rect` until dropped.
pub struct KeepOff {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl KeepOff {
    pub fn start(rect: Rect) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let thread = std::thread::Builder::new()
            .name("roam-keep-off".into())
            .spawn(move || {
                while !stopping.load(Ordering::Acquire) {
                    if let Some(to) = elsewhere(rect) {
                        sweep(rect, to);
                    }
                    std::thread::sleep(EVERY);
                }
            })
            .ok();
        Self { stop, thread }
    }
}

impl Drop for KeepOff {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        // Not joined: a slow app may hold the thread a moment, and it ends by itself.
        drop(self.thread.take());
    }
}

/// The biggest of the Mac's other screens, where windows go instead.
fn elsewhere(lent: Rect) -> Option<Rect> {
    let mut ids = [0u32; 16];
    let mut count = 0u32;
    // SAFETY: room for 16 displays, as the call is told.
    if unsafe { CGGetActiveDisplayList(16, ids.as_mut_ptr(), &mut count) } != 0 {
        return None;
    }
    ids[..count.min(16) as usize]
        .iter()
        // SAFETY: plain queries of displays the system just listed.
        .map(|&id| unsafe { CGDisplayBounds(id) })
        .map(|b| Rect::new(b.origin.x, b.origin.y, b.size.width, b.size.height))
        .filter(|r| !overlaps_mostly(*r, lent))
        .max_by(|a, b| (a.w * a.h).total_cmp(&(b.w * b.h)))
}

/// Whether most of `a` lies on `b`.
fn overlaps_mostly(a: Rect, b: Rect) -> bool {
    let w = (a.right().min(b.right()) - a.x.max(b.x)).max(0.0);
    let h = (a.bottom().min(b.bottom()) - a.y.max(b.y)).max(0.0);
    w * h * 2.0 > a.w * a.h
}

/// Where a window at `window` goes on `to`: its top left in from the corner, and its size cut
/// down to fit. `None` when it already fits as it is (it only needs moving).
fn landing(window: Rect, to: Rect) -> (Point, Option<Size>) {
    let room = Size { width: (to.w - 2.0 * INSET).max(200.0), height: (to.h - 2.0 * INSET).max(150.0) };
    let size = Size { width: window.w.min(room.width), height: window.h.min(room.height) };
    let resized = (size.width < window.w || size.height < window.h).then_some(size);
    (Point { x: to.x + INSET, y: to.y + INSET }, resized)
}

/// The apps with an ordinary window on `lent`, other than this one.
fn apps_on(lent: Rect) -> Vec<i32> {
    let mine = std::process::id() as i32;
    let mut pids = Vec::new();
    // SAFETY: the list is owned here and released below; every value read from it is borrowed
    // from the list and checked before use.
    unsafe {
        let list = CGWindowListCopyWindowInfo(ON_SCREEN_ONLY | EXCLUDE_DESKTOP, 0);
        if list.is_null() {
            return pids;
        }
        for index in 0..CFArrayGetCount(list) {
            let window = CFArrayGetValueAtIndex(list, index);
            let number = |key: Ref| {
                let value = CFDictionaryGetValue(window, key);
                let mut out = 0i32;
                (!value.is_null() && CFNumberGetValue(value, NUMBER_SINT32, (&mut out as *mut i32).cast()) != 0)
                    .then_some(out)
            };
            // Layer 0 holds ordinary app windows; menus, the Dock and Roam's own cover sit higher.
            let (Some(pid), Some(0)) = (number(kCGWindowOwnerPID), number(kCGWindowLayer)) else { continue };
            let bounds = CFDictionaryGetValue(window, kCGWindowBounds);
            let mut b = Bounds::default();
            if pid == mine || bounds.is_null() || CGRectMakeWithDictionaryRepresentation(bounds, &mut b) == 0 {
                continue;
            }
            let rect = Rect::new(b.origin.x, b.origin.y, b.size.width, b.size.height);
            if rect.w >= 1.0 && rect.h >= 1.0 && overlaps_mostly(rect, lent) && !pids.contains(&pid) {
                pids.push(pid);
            }
        }
        CFRelease(list);
    }
    pids
}

/// Moves every window of the apps on `lent` that sits mostly there over to `to`.
fn sweep(lent: Rect, to: Rect) {
    let windows = CFString::from_static_str("AXWindows");
    let position = CFString::from_static_str("AXPosition");
    let size = CFString::from_static_str("AXSize");
    let key = |s: &CFString| (s as *const CFString).cast::<c_void>().cast_mut();
    for pid in apps_on(lent) {
        // SAFETY: AX objects made or copied here are released here; values are checked before
        // they're read.
        unsafe {
            let app = AXUIElementCreateApplication(pid);
            if app.is_null() {
                continue;
            }
            AXUIElementSetMessagingTimeout(app, PATIENCE);
            let mut list: Ref = std::ptr::null_mut();
            if AXUIElementCopyAttributeValue(app, key(&windows), &mut list) == 0 && !list.is_null() {
                for index in 0..CFArrayGetCount(list) {
                    let window = CFArrayGetValueAtIndex(list, index);
                    let Some(at) = read::<Point>(window, key(&position), AX_VALUE_POINT) else { continue };
                    let Some(extent) = read::<Size>(window, key(&size), AX_VALUE_SIZE) else { continue };
                    let rect = Rect::new(at.x, at.y, extent.width, extent.height);
                    if !overlaps_mostly(rect, lent) {
                        continue;
                    }
                    let (to_at, resized) = landing(rect, to);
                    if let Some(resized) = resized {
                        write(window, key(&size), AX_VALUE_SIZE, &resized);
                    }
                    write(window, key(&position), AX_VALUE_POINT, &to_at);
                }
                CFRelease(list);
            }
            CFRelease(app);
        }
    }
}

/// An AX point or size attribute of `element`.
unsafe fn read<T: Default>(element: Ref, attribute: Ref, kind: u32) -> Option<T> {
    let mut value: Ref = std::ptr::null_mut();
    // SAFETY: the value is copied, read as the kind asked, and released.
    unsafe {
        if AXUIElementCopyAttributeValue(element, attribute, &mut value) != 0 || value.is_null() {
            return None;
        }
        let mut out = T::default();
        let ok = AXValueGetValue(value, kind, (&mut out as *mut T).cast()) != 0;
        CFRelease(value);
        ok.then_some(out)
    }
}

unsafe fn write<T>(element: Ref, attribute: Ref, kind: u32, value: &T) {
    // SAFETY: an AX value of the kind given, made from `value` and released after use.
    unsafe {
        let made = AXValueCreate(kind, (value as *const T).cast());
        if !made.is_null() {
            AXUIElementSetAttributeValue(element, attribute, made);
            CFRelease(made);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_too_big_for_the_other_screen_is_made_smaller() {
        let to = Rect::new(-1512.0, 0.0, 1512.0, 982.0);
        let (at, size) = landing(Rect::new(100.0, 50.0, 2400.0, 1300.0), to);
        assert_eq!((at.x, at.y), (-1472.0, 40.0));
        let size = size.unwrap();
        assert_eq!((size.width, size.height), (1432.0, 902.0));
        let (_, same) = landing(Rect::new(100.0, 50.0, 800.0, 600.0), to);
        assert!(same.is_none(), "a window that fits keeps its size");
    }

    #[test]
    fn mostly_means_more_than_half() {
        let lent = Rect::new(0.0, 0.0, 2560.0, 1440.0);
        assert!(overlaps_mostly(Rect::new(2000.0, 100.0, 400.0, 300.0), lent));
        assert!(!overlaps_mostly(Rect::new(2400.0, 100.0, 400.0, 300.0), lent));
    }
}
