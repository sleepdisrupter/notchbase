//! Joining a COM apartment for the audio thread, and taking strings COM hands back.

use std::ffi::c_void;

use ::windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoTaskMemFree, CoUninitialize};
use ::windows::core::{GUID, PWSTR};

// Spells "notchbase-mixer!". Volume changes we make carry it, so listeners can tell they came from us.
pub const EVENT_CONTEXT: GUID = GUID::from_u128(0x6e6f7463_6862_6173_652d_6d6978657221);

pub struct Apartment {
    joined: bool,
}

impl Apartment {
    pub fn join_multithreaded() -> Self {
        let joined = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
        Self { joined }
    }

    pub fn is_joined(&self) -> bool {
        self.joined
    }
}

impl Drop for Apartment {
    fn drop(&mut self) {
        if self.joined {
            unsafe { CoUninitialize() };
        }
    }
}

pub unsafe fn take_string(text: PWSTR) -> Option<String> {
    if text.is_null() {
        return None;
    }
    let owned = String::from_utf16_lossy(unsafe { text.as_wide() });
    unsafe { CoTaskMemFree(Some(text.0 as *const c_void)) };
    Some(owned)
}
