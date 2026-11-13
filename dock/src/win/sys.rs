//! Small Win32 pieces the rest of `win/` shares: window handles as the dock's window ids, the windows that count as
//! apps, a process's program file, and window frames as DWM draws them.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use windows_sys::Win32::Foundation::{CloseHandle, HWND, LPARAM, RECT, TRUE};
use windows_sys::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, EnumWindows, GA_ROOTOWNER, GW_OWNER, GWL_EXSTYLE, GWL_STYLE, GetAncestor, GetClassNameW,
    GetLastActivePopup, GetWindow, GetWindowLongPtrW, GetWindowPlacement, GetWindowRect, GetWindowTextLengthW,
    GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindowVisible, SW_SHOWMAXIMIZED, WINDOWPLACEMENT, WS_CHILD,
    WS_EX_APPWINDOW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};
use windows_sys::core::BOOL;

/// A window id as the dock keeps it. Window handles only ever use their low 32 bits, so they round-trip.
pub fn id_of(hwnd: HWND) -> u32 {
    hwnd as usize as u32
}

pub fn hwnd_of(id: u32) -> HWND {
    id as i32 as isize as HWND
}

pub fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

pub fn wide_path(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt as _;
    path.as_os_str().encode_wide().chain([0]).collect()
}

pub fn class_name(hwnd: HWND) -> String {
    let mut buffer = [0u16; 256];
    let len = unsafe { GetClassNameW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32) };
    String::from_utf16_lossy(&buffer[..len.max(0) as usize])
}

pub fn title_of(hwnd: HWND) -> String {
    let len = unsafe { GetWindowTextLengthW(hwnd) };
    if len <= 0 {
        return String::new();
    }
    let mut buffer = vec![0u16; len as usize + 1];
    let got = unsafe { GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32) };
    String::from_utf16_lossy(&buffer[..got.max(0) as usize])
}

/// The process that owns a window. A UWP app's frame belongs to ApplicationFrameHost; the app is the process behind
/// the core window inside it.
pub fn pid_of(hwnd: HWND) -> i32 {
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    if program_of(pid as i32).is_some_and(|p| ends_with(&p, "applicationframehost.exe"))
        && let Some(inner) = core_window_pid(hwnd, pid)
    {
        return inner as i32;
    }
    pid as i32
}

fn ends_with(path: &Path, name: &str) -> bool {
    path.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(name))
}

fn core_window_pid(frame: HWND, host: u32) -> Option<u32> {
    struct Find {
        host: u32,
        found: Option<u32>,
    }
    unsafe extern "system" fn visit(child: HWND, find: LPARAM) -> BOOL {
        let find = unsafe { &mut *(find as *mut Find) };
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(child, &mut pid) };
        if pid != find.host && class_name(child) == "Windows.UI.Core.CoreWindow" {
            find.found = Some(pid);
            return 0;
        }
        TRUE
    }
    let mut find = Find { host, found: None };
    unsafe { EnumChildWindows(frame, Some(visit), (&raw mut find) as LPARAM) };
    find.found
}

/// A process's program file, remembered per pid for a while (pids come back for new processes).
pub fn program_of(pid: i32) -> Option<PathBuf> {
    type Known = HashMap<i32, (Option<PathBuf>, std::time::Instant)>;
    static KNOWN: Mutex<Option<Known>> = Mutex::new(None);
    if pid <= 0 {
        return None;
    }
    let mut known = KNOWN.lock().unwrap_or_else(|e| e.into_inner());
    let known = known.get_or_insert_with(HashMap::new);
    if let Some((path, at)) = known.get(&pid)
        && at.elapsed().as_secs() < 30
    {
        return path.clone();
    }
    let path = read_program(pid as u32);
    if known.len() > 2048 {
        known.clear();
    }
    known.insert(pid, (path.clone(), std::time::Instant::now()));
    path
}

fn read_program(pid: u32) -> Option<PathBuf> {
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return None;
    }
    let mut path = vec![0u16; 1024];
    let mut length = path.len() as u32;
    let read = unsafe { QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, path.as_mut_ptr(), &mut length) };
    unsafe { CloseHandle(process) };
    (read != 0).then(|| PathBuf::from(String::from_utf16_lossy(&path[..length as usize])))
}

/// Whether DWM keeps a window out of sight: a suspended UWP app's frame, or a window on another desktop.
pub fn cloaked(hwnd: HWND) -> bool {
    let mut value: u32 = 0;
    let got =
        unsafe { DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED as u32, (&raw mut value).cast(), size_of::<u32>() as u32) };
    got == 0 && value != 0
}

/// Whether a top-level window is one a person thinks of as an app's window: what the taskbar would show a button
/// for. Minimized ones count.
pub fn is_app_window(hwnd: HWND) -> bool {
    if unsafe { IsWindowVisible(hwnd) } == 0 || cloaked(hwnd) {
        return false;
    }
    let style = unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) } as u32;
    let ex = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32;
    if style & WS_CHILD != 0 || ex & WS_EX_TOOLWINDOW != 0 {
        return false;
    }
    let owned = !unsafe { GetWindow(hwnd, GW_OWNER) }.is_null();
    if owned && ex & WS_EX_APPWINDOW == 0 {
        return false;
    }
    if ex & WS_EX_NOACTIVATE != 0 && ex & WS_EX_APPWINDOW == 0 {
        return false;
    }
    // A window that's the last active popup of its own root owner is the one Alt+Tab shows.
    let root = unsafe { GetAncestor(hwnd, GA_ROOTOWNER) };
    if !root.is_null() && root != hwnd && unsafe { GetLastActivePopup(root) } != hwnd && ex & WS_EX_APPWINDOW == 0 {
        return false;
    }
    !matches!(class_name(hwnd).as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd")
}

/// Every app window, front to back.
pub fn app_windows() -> Vec<HWND> {
    unsafe extern "system" fn visit(hwnd: HWND, found: LPARAM) -> BOOL {
        if is_app_window(hwnd) {
            unsafe { &mut *(found as *mut Vec<HWND>) }.push(hwnd);
        }
        TRUE
    }
    let mut found: Vec<HWND> = Vec::new();
    unsafe { EnumWindows(Some(visit), (&raw mut found) as LPARAM) };
    found
}

pub fn iconic(hwnd: HWND) -> bool {
    unsafe { IsIconic(hwnd) != 0 }
}

/// A window's frame as drawn, without the invisible resize borders, in physical pixels.
pub fn frame(hwnd: HWND) -> Option<RECT> {
    let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    let got = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS as u32,
            (&raw mut rect).cast(),
            size_of::<RECT>() as u32,
        )
    };
    if got != 0 && unsafe { GetWindowRect(hwnd, &mut rect) } == 0 {
        return None;
    }
    (rect.right > rect.left && rect.bottom > rect.top).then_some(rect)
}

/// The window's whole rectangle, borders included, in physical pixels.
pub fn outer(hwnd: HWND) -> Option<RECT> {
    let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    (unsafe { GetWindowRect(hwnd, &mut rect) } != 0).then_some(rect)
}

/// Where a minimized window goes back to, and whether it comes back maximized.
pub fn placement(hwnd: HWND) -> Option<(RECT, bool)> {
    let mut place: WINDOWPLACEMENT = unsafe { std::mem::zeroed() };
    place.length = size_of::<WINDOWPLACEMENT>() as u32;
    if unsafe { GetWindowPlacement(hwnd, &mut place) } == 0 {
        return None;
    }
    // WPF_RESTORETOMAXIMIZED
    let maximized = place.flags & 2 != 0 || place.showCmd == SW_SHOWMAXIMIZED as u32;
    Some((place.rcNormalPosition, maximized))
}

pub fn rect_of(r: RECT) -> egui::Rect {
    egui::Rect::from_min_max(egui::pos2(r.left as f32, r.top as f32), egui::pos2(r.right as f32, r.bottom as f32))
}
