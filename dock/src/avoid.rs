//! Keeps zoomed, filled and tiled windows clear of the dock. With the system Dock hidden, macOS lets a zoomed window
//! run to the edge of the screen, under this dock. A few times a second this looks at the front app's focused window
//! through Accessibility; one that reaches the edge the dock stands on (which is what zoom, Fill and tiling do) is
//! pulled back to end just clear of the dock. Windows being dragged, full-screen windows and ones placed short of the
//! edge are left alone.

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
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(cf: *const c_void);
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

    /// Looks at `pid`'s focused window and, if it runs under the dock on whichever of `areas` it's on (to the
    /// screen's edge the dock stands on), pulls it back to end just clear of it. Returns true when it moved one.
    pub fn check(&mut self, pid: i32, areas: &[crate::titlebar::Area]) -> bool {
        if !self.trusted() {
            return false;
        }
        let app = unsafe { AXUIElementCreateApplication(pid) };
        if app.is_null() {
            return false;
        }
        let moved = copy(app, ns_string!("AXFocusedWindow")).is_some_and(|window| {
            let moved = crate::titlebar::fit_if_filled(window, areas);
            unsafe { CFRelease(window) };
            moved
        });
        unsafe { CFRelease(app) };
        moved
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
