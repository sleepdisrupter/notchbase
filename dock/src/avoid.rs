//! Keeps zoomed, filled and tiled windows above the dock. With the system Dock hidden, macOS lets a zoomed window run
//! to the bottom of the screen, under this dock. A few times a second this looks at the front app's focused window
//! through Accessibility; one that reaches the bottom edge (which is what zoom, Fill and tiling do) is shortened so its
//! bottom sits just above the dock. Windows being dragged, full-screen windows and ones placed short of the bottom are
//! left alone.

use std::ffi::c_void;

use objc2::rc::Retained;
use objc2_foundation::{NSDictionary, NSNumber, NSString, ns_string};

type AxElement = *const c_void;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
    fn AXUIElementCreateApplication(pid: i32) -> AxElement;
    fn AXUIElementCopyAttributeValue(element: AxElement, attribute: *const c_void, value: *mut *const c_void) -> i32;
    fn AXUIElementSetAttributeValue(element: AxElement, attribute: *const c_void, value: *const c_void) -> i32;
    fn AXValueCreate(kind: u32, value: *const c_void) -> *const c_void;
    fn AXValueGetValue(value: *const c_void, kind: u32, out: *mut c_void) -> bool;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(cf: *const c_void);
    fn CFBooleanGetValue(boolean: *const c_void) -> bool;
    fn CFGetTypeID(cf: *const c_void) -> usize;
    fn CFBooleanGetTypeID() -> usize;
}

const POINT: u32 = 1;
const SIZE: u32 = 2;

#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq)]
struct Xy {
    x: f64,
    y: f64,
}

/// Asks once for Accessibility (macOS shows its own prompt), then just checks.
pub struct Avoider {
    asked: bool,
}

impl Avoider {
    pub fn new() -> Self {
        Self { asked: false }
    }

    fn trusted(&mut self) -> bool {
        if unsafe { AXIsProcessTrusted() } {
            return true;
        }
        if !self.asked {
            self.asked = true;
            let key = ns_string!("AXTrustedCheckOptionPrompt");
            let yes = NSNumber::new_bool(true);
            let options: Retained<NSDictionary<NSString, NSNumber>> = NSDictionary::from_slices(&[key], &[&*yes]);
            unsafe { AXIsProcessTrustedWithOptions(Retained::as_ptr(&options) as *const c_void) };
        }
        false
    }

    /// Looks at `pid`'s focused window and, on whichever of `docks` it's on, lifts its bottom to that dock's `limit`
    /// (global top-left points) if it reaches down to the screen's bottom. Each is (limit, screen bottom, left,
    /// right), the screen across in global points. Returns true when it moved one.
    pub fn check(&mut self, pid: i32, docks: &[(f64, f64, f64, f64)]) -> bool {
        if !self.trusted() {
            return false;
        }
        let app = unsafe { AXUIElementCreateApplication(pid) };
        if app.is_null() {
            return false;
        }
        let moved = (|| {
            let window = copy(app, ns_string!("AXFocusedWindow"))?;
            let result = (|| {
                if copy_bool(window, ns_string!("AXFullScreen")) == Some(true) {
                    return None;
                }
                let at = copy_value(window, ns_string!("AXPosition"), POINT)?;
                let size = copy_value(window, ns_string!("AXSize"), SIZE)?;
                let bottom = at.y + size.y;
                let middle = at.x + size.x * 0.5;
                let &(limit, screen_bottom, _, _) =
                    docks.iter().find(|(_, _, left, right)| (*left..*right).contains(&middle))?;
                // Only windows run to the bottom edge, as zoom, Fill and tiling do, and only if they're under the dock.
                if bottom < screen_bottom - 2.0 || bottom <= limit + 1.0 || at.y >= limit - 120.0 {
                    return None;
                }
                let wanted = Xy { x: size.x, y: limit - at.y };
                let value = unsafe { AXValueCreate(SIZE, &wanted as *const Xy as *const c_void) };
                if value.is_null() {
                    return None;
                }
                let error = unsafe { AXUIElementSetAttributeValue(window, attr(ns_string!("AXSize")), value) };
                unsafe { CFRelease(value) };
                Some(error == 0)
            })();
            unsafe { CFRelease(window) };
            result
        })();
        unsafe { CFRelease(app) };
        moved == Some(true)
    }
}

fn attr(name: &NSString) -> *const c_void {
    name as *const NSString as *const c_void
}

fn copy(element: AxElement, name: &NSString) -> Option<AxElement> {
    let mut out = std::ptr::null();
    let error = unsafe { AXUIElementCopyAttributeValue(element, attr(name), &mut out) };
    (error == 0 && !out.is_null()).then_some(out)
}

fn copy_value(element: AxElement, name: &NSString, kind: u32) -> Option<Xy> {
    let value = copy(element, name)?;
    let mut out = Xy::default();
    let ok = unsafe { AXValueGetValue(value, kind, &mut out as *mut Xy as *mut c_void) };
    unsafe { CFRelease(value) };
    ok.then_some(out)
}

fn copy_bool(element: AxElement, name: &NSString) -> Option<bool> {
    let value = copy(element, name)?;
    let result =
        (unsafe { CFGetTypeID(value) } == unsafe { CFBooleanGetTypeID() }).then(|| unsafe { CFBooleanGetValue(value) });
    unsafe { CFRelease(value) };
    result
}
