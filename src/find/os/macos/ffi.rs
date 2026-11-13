//! The few CoreServices and dispatch types the FSEvents code needs.

use std::ffi::{c_char, c_int, c_void};

pub const IOPOL_TYPE_DISK: c_int = 0;
pub const IOPOL_TYPE_VFS_MATERIALIZE_DATALESS_FILES: c_int = 3;
pub const IOPOL_SCOPE_THREAD: c_int = 1;
pub const IOPOL_UTILITY: c_int = 4;
pub const IOPOL_MATERIALIZE_DATALESS_FILES_OFF: c_int = 1;
pub const SF_DATALESS: u32 = 0x4000_0000;
pub const ATTR_CMN_ERROR: u32 = 0x2000_0000;
pub const VDIR: u32 = 2;
pub const VLNK: u32 = 5;

pub type DispatchQueue = *mut c_void;

unsafe extern "C" {
    pub fn setiopolicy_np(kind: c_int, scope: c_int, policy: c_int) -> c_int;
    pub fn dispatch_queue_create(label: *const c_char, attributes: *const c_void) -> DispatchQueue;
    pub fn dispatch_release(object: DispatchQueue);
}
