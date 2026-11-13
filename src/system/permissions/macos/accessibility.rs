//! The Accessibility check and its system prompt.

use std::ffi::c_void;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_foundation::{NSDictionary, NSNumber, NSString};

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
    static kAXTrustedCheckOptionPrompt: *const c_void;
}

pub fn trusted() -> bool {
    unsafe { AXIsProcessTrusted() }
}

pub fn prompt() {
    let Some(key) = (unsafe { kAXTrustedCheckOptionPrompt.cast::<NSString>().as_ref() }) else { return };
    let yes = NSNumber::new_bool(true);
    let values: [&AnyObject; 1] = [&yes];
    let options: Retained<NSDictionary<NSString, AnyObject>> = NSDictionary::from_slices(&[key], &values);
    unsafe { AXIsProcessTrustedWithOptions(Retained::as_ptr(&options).cast()) };
}
