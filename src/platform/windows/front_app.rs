//! The foreground window, kept so it can be put back after a capture or typing in the notch, and the front app's name.

use std::path::Path;

use windows_sys::Win32::Foundation::{CloseHandle, HWND};
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowThreadProcessId, IsWindow, SetForegroundWindow,
};

const PATH_ROOM: usize = 1024;

pub struct FrontApp(HWND);

pub fn remember_front_app() -> Option<FrontApp> {
    let hwnd = unsafe { GetForegroundWindow() };
    (!hwnd.is_null()).then_some(FrontApp(hwnd))
}

pub fn restore_front_app(app: FrontApp) {
    unsafe {
        if IsWindow(app.0) != 0 {
            SetForegroundWindow(app.0);
        }
    }
}

pub fn running_apps() -> Vec<std::path::PathBuf> {
    Vec::new()
}

pub fn front_app_name() -> Option<String> {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_null() {
        return None;
    }
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    (pid != 0 && pid != std::process::id()).then(|| program_name(pid)).flatten()
}

fn program_name(pid: u32) -> Option<String> {
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return None;
    }
    let mut path = [0u16; PATH_ROOM];
    let mut length = PATH_ROOM as u32;
    let read = unsafe { QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, path.as_mut_ptr(), &mut length) };
    unsafe { CloseHandle(process) };
    let path = String::from_utf16_lossy(&path[..(length as usize).min(PATH_ROOM)]);
    (read != 0).then(|| Path::new(&path).file_stem().map(|stem| stem.to_string_lossy().into_owned())).flatten()
}
