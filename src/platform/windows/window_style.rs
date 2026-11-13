//! Changes a window's extended style in place. A newly layered window is set opaque, so GL's own alpha still applies.

use raw_window_handle::{HasWindowHandle as _, RawWindowHandle};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetWindowLongPtrW, LWA_ALPHA, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
    SetLayeredWindowAttributes, SetWindowLongPtrW, SetWindowPos, WS_EX_LAYERED,
};
use winit::window::Window;

const OPAQUE: u8 = 255;

pub fn hwnd(window: &Window) -> Option<HWND> {
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get() as HWND),
        _ => None,
    }
}

pub fn update_ex_style(window: &Window, add: u32, remove: u32) {
    let Some(hwnd) = hwnd(window) else { return };
    let old = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32;
    let new = (old | add) & !remove;
    if new != old {
        replace_ex_style(hwnd, new);
    }
    if new & WS_EX_LAYERED != 0 && old & WS_EX_LAYERED == 0 {
        unsafe { SetLayeredWindowAttributes(hwnd, 0, OPAQUE, LWA_ALPHA) };
    }
}

fn replace_ex_style(hwnd: HWND, style: u32) {
    let keep_geometry = SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED;
    unsafe {
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style as isize);
        SetWindowPos(hwnd, std::ptr::null_mut(), 0, 0, 0, 0, keep_geometry);
    }
}
