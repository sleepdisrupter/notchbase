//! The windows that count as apps: visible, unowned and not tool windows. Also the shell's own process.

use std::collections::HashSet;

use windows_sys::Win32::Foundation::{HWND, LPARAM, TRUE};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GW_OWNER, GWL_EXSTYLE, GetShellWindow, GetWindow, GetWindowLongPtrW, GetWindowThreadProcessId,
    IsWindowVisible, WS_EX_TOOLWINDOW,
};
use windows_sys::core::BOOL;

pub(super) struct AppWindow {
    pub(super) hwnd: HWND,
    pub(super) pid: u32,
}

pub(super) fn app_windows() -> Vec<AppWindow> {
    let mut found: Vec<AppWindow> = Vec::new();
    unsafe { EnumWindows(Some(visit), (&raw mut found) as LPARAM) };
    found
}

pub fn windowed_processes() -> HashSet<u32> {
    app_windows().into_iter().map(|window| window.pid).collect()
}

pub fn shell_process() -> Option<u32> {
    let shell = unsafe { GetShellWindow() };
    (!shell.is_null()).then(|| process_of(shell)).filter(|pid| *pid != 0)
}

unsafe extern "system" fn visit(hwnd: HWND, found: LPARAM) -> BOOL {
    if is_app_window(hwnd) {
        let found = unsafe { &mut *(found as *mut Vec<AppWindow>) };
        found.push(AppWindow { hwnd, pid: process_of(hwnd) });
    }
    TRUE
}

fn is_app_window(hwnd: HWND) -> bool {
    let style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32;
    let visible = unsafe { IsWindowVisible(hwnd) } != 0;
    let owned = !unsafe { GetWindow(hwnd, GW_OWNER) }.is_null();
    visible && !owned && style & WS_EX_TOOLWINDOW == 0
}

fn process_of(hwnd: HWND) -> u32 {
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    pid
}
