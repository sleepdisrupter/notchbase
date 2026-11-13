//! Looks up a function at runtime, for APIs that older systems don't have.

use std::ffi::{CStr, c_char, c_void};
use std::ptr::NonNull;

const RTLD_DEFAULT: *mut c_void = -2isize as *mut c_void;

unsafe extern "C" {
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}

pub fn global_symbol(name: &CStr) -> Option<NonNull<c_void>> {
    NonNull::new(unsafe { dlsym(RTLD_DEFAULT, name.as_ptr()) })
}
