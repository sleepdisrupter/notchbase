//! The lock state on macOS, from the current session's dictionary.

use std::ffi::c_void;

use objc2::msg_send;
use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::AnyObject;
use objc2_foundation::NSString;

const SCREEN_IS_LOCKED: &str = "CGSSessionScreenIsLocked";

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGSessionCopyCurrentDictionary() -> *mut c_void;
}

pub(super) fn locked() -> bool {
    autoreleasepool(|_| {
        let Some(session) = (unsafe { Retained::from_raw(CGSessionCopyCurrentDictionary().cast::<AnyObject>()) })
        else {
            return false;
        };
        let flag: Option<Retained<AnyObject>> =
            unsafe { msg_send![&*session, objectForKey: &*NSString::from_str(SCREEN_IS_LOCKED)] };
        flag.is_some_and(|flag| unsafe { msg_send![&*flag, boolValue] })
    })
}
