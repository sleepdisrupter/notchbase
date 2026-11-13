//! On Windows, Do Not Disturb is read from the quiet hours state through an undocumented ntdll call.

use std::ffi::c_void;
use std::ptr::null;

use notchbase_sdk::protocol::Focus;
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};

use super::Reading;

const QUIET_HOURS_PROFILE: u64 = 0x0D83_063E_A3BF_1C75;
const DO_NOT_DISTURB: &str = "Do Not Disturb";

type QueryState =
    unsafe extern "system" fn(*const u64, *const c_void, *const c_void, *mut u32, *mut c_void, *mut u32) -> i32;

pub(super) fn read() -> Reading {
    let active = profile().is_some_and(|profile| profile != 0);
    Reading::Seen(Focus { active, name: active.then(|| DO_NOT_DISTURB.to_owned()) })
}

fn profile() -> Option<u32> {
    let library: Vec<u16> = "ntdll.dll".encode_utf16().chain([0]).collect();
    let module = unsafe { GetModuleHandleW(library.as_ptr()) };
    if module.is_null() {
        return None;
    }
    let found = unsafe { GetProcAddress(module, c"NtQueryWnfStateData".as_ptr().cast()) }?;
    let query = unsafe { std::mem::transmute::<unsafe extern "system" fn() -> isize, QueryState>(found) };
    let (mut stamp, mut profile, mut size) = (0u32, 0u32, size_of::<u32>() as u32);
    let state = QUIET_HOURS_PROFILE;
    let status = unsafe { query(&state, null(), null(), &mut stamp, (&raw mut profile).cast(), &mut size) };
    (status >= 0 && size as usize >= size_of::<u32>()).then_some(profile)
}
