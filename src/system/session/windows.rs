//! The lock state on Windows, from the session information.

use std::ptr::null_mut;

use windows_sys::Win32::System::RemoteDesktop::{
    WTS_CURRENT_SERVER_HANDLE, WTS_CURRENT_SESSION, WTS_SESSIONSTATE_LOCK, WTSFreeMemory, WTSINFOEXW,
    WTSQuerySessionInformationW, WTSSessionInfoEx,
};

const LEVEL_ONE: u32 = 1;

pub(super) fn locked() -> bool {
    let mut buffer: *mut u16 = null_mut();
    let mut bytes = 0u32;
    let asked = unsafe {
        WTSQuerySessionInformationW(
            WTS_CURRENT_SERVER_HANDLE,
            WTS_CURRENT_SESSION,
            WTSSessionInfoEx,
            &mut buffer,
            &mut bytes,
        )
    };
    if asked == 0 || buffer.is_null() {
        return false;
    }
    let locked = bytes as usize >= size_of::<WTSINFOEXW>() && {
        let info = unsafe { buffer.cast::<WTSINFOEXW>().read_unaligned() };
        info.Level == LEVEL_ONE && unsafe { info.Data.WTSInfoExLevel1.SessionFlags } as u32 == WTS_SESSIONSTATE_LOCK
    };
    unsafe { WTSFreeMemory(buffer.cast()) };
    locked
}
