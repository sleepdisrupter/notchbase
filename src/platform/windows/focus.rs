//! Brings a process's window to the front, borrowing the foreground thread's input when Windows won't allow it.

use windows_sys::Win32::Foundation::{FALSE, HWND, TRUE};
use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId, IsIconic, SW_RESTORE, SetForegroundWindow,
    ShowWindow,
};

use super::app_windows::app_windows;

pub fn focus_window(pid: u32) -> bool {
    let Some(window) = app_windows().into_iter().find(|window| window.pid == pid) else {
        return false;
    };
    let hwnd = window.hwnd;
    if unsafe { IsIconic(hwnd) } != 0 {
        unsafe { ShowWindow(hwnd, SW_RESTORE) };
    }
    if !bring_forward(hwnd) {
        borrow_input(hwnd);
    }
    unsafe { GetForegroundWindow() == hwnd }
}

fn bring_forward(hwnd: HWND) -> bool {
    unsafe { SetForegroundWindow(hwnd) != 0 && GetForegroundWindow() == hwnd }
}

fn borrow_input(hwnd: HWND) {
    let front = unsafe { GetWindowThreadProcessId(GetForegroundWindow(), std::ptr::null_mut()) };
    let mine = unsafe { GetCurrentThreadId() };
    let attached = front != 0 && front != mine && unsafe { AttachThreadInput(mine, front, TRUE) } != 0;
    unsafe {
        BringWindowToTop(hwnd);
        SetForegroundWindow(hwnd);
    }
    if attached {
        unsafe { AttachThreadInput(mine, front, FALSE) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windowless_process_is_left_alone() {
        assert!(!focus_window(u32::MAX));
    }

    #[test]
    #[ignore = "brings the window of NOTCHBASE_FOCUS_PID to the front"]
    fn focuses_a_real_process() {
        let pid = std::env::var("NOTCHBASE_FOCUS_PID").ok().and_then(|pid| pid.parse().ok()).expect("a pid");
        assert!(focus_window(pid));
    }
}
