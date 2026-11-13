//! Window layouts on Windows, in physical pixels. Frames are the visible ones DWM reports, without the invisible
//! resize borders, and `SetWindowPos` moves the windows.

use std::mem::size_of;

use egui::{Pos2, Rect, pos2, vec2};
use windows_sys::Win32::Foundation::{ERROR_ACCESS_DENIED, GetLastError, HWND, LPARAM, POINT, RECT, TRUE};
use windows_sys::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
use windows_sys::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO};
use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GA_ROOT, GWL_EXSTYLE, GWL_STYLE, GetAncestor, GetClassNameW, GetCursorPos, GetShellWindow,
    GetWindowLongPtrW, GetWindowRect, GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible, IsZoomed,
    SW_RESTORE, SWP_NOACTIVATE, SWP_NOOWNERZORDER, SWP_NOZORDER, SetWindowPos, ShowWindow, WS_CAPTION,
    WS_EX_TOOLWINDOW, WS_THICKFRAME, WindowFromPoint,
};
use windows_sys::core::BOOL;
use winit::monitor::MonitorHandle;

use super::{Grab, Handle, Screen};

// Anything smaller is too small to be a window worth laying out.
const MIN_WIDTH: f32 = 150.0;
const MIN_HEIGHT: f32 = 100.0;
const BASE_DPI: f32 = 96.0;
/// The desktop and taskbar windows, which are never laid out.
const SHELL_CLASSES: [&str; 4] = ["Progman", "WorkerW", "Shell_TrayWnd", "Shell_SecondaryTrayWnd"];

pub(super) fn allowed() -> bool {
    true
}

pub(super) fn ask_for_access() {}

pub(super) fn cursor() -> Option<Pos2> {
    let mut point = POINT { x: 0, y: 0 };
    (unsafe { GetCursorPos(&mut point) } != 0).then(|| pos2(point.x as f32, point.y as f32))
}

pub(super) fn screens() -> Vec<Screen> {
    let mut found: Vec<Screen> = Vec::new();
    unsafe {
        EnumDisplayMonitors(std::ptr::null_mut(), std::ptr::null(), Some(visit_monitor), (&raw mut found) as LPARAM)
    };
    found
}

pub(super) fn monitor_key(monitor: &MonitorHandle) -> u64 {
    let origin = monitor.position();
    key_of(origin.x, origin.y)
}

pub(super) fn window_at(point: Pos2) -> Option<Grab> {
    let at = POINT { x: point.x.round() as i32, y: point.y.round() as i32 };
    let hit = unsafe { WindowFromPoint(at) };
    if hit.is_null() {
        return None;
    }
    let root = unsafe { GetAncestor(hit, GA_ROOT) };
    let hwnd = if root.is_null() { hit } else { root };
    if !is_candidate(hwnd) {
        return None;
    }
    grab_of(hwnd)
}

pub(super) fn frame_of(handle: Handle) -> Option<Rect> {
    let hwnd = handle.id as HWND;
    if unsafe { IsWindow(hwnd) } == 0 {
        return None;
    }
    visible_frame(hwnd)
}

pub(super) fn windows_on(screen: &Screen) -> Vec<Grab> {
    let mut found: Vec<HWND> = Vec::new();
    unsafe { EnumWindows(Some(visit_window), (&raw mut found) as LPARAM) };
    found
        .into_iter()
        .filter(|hwnd| is_candidate(*hwnd) && is_arrangeable(*hwnd))
        .filter_map(grab_of)
        .filter(|grab| {
            grab.frame.width() >= MIN_WIDTH
                && grab.frame.height() >= MIN_HEIGHT
                && screen.frame.contains(grab.frame.center())
        })
        .collect()
}

pub(super) fn place(handle: Handle, frame: Rect) -> Result<(), String> {
    let hwnd = handle.id as HWND;
    if unsafe { IsWindow(hwnd) } == 0 {
        return Err("That window is gone".into());
    }
    // A maximized window is restored first, so it takes the new frame as an ordinary window.
    if unsafe { IsZoomed(hwnd) } != 0 {
        unsafe { ShowWindow(hwnd, SW_RESTORE) };
    }
    let outer = window_rect(hwnd).ok_or("Couldn't move that window")?;
    let visible = visible_frame(hwnd).unwrap_or(outer);
    let target = outer_target(frame, outer, visible);
    let flags = SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER;
    let (x, y) = (target.left().round() as i32, target.top().round() as i32);
    let (w, h) = (target.width().round() as i32, target.height().round() as i32);
    if unsafe { SetWindowPos(hwnd, std::ptr::null_mut(), x, y, w, h, flags) } != 0 {
        return Ok(());
    }
    Err(match unsafe { GetLastError() } {
        ERROR_ACCESS_DENIED => "That window needs administrator rights".into(),
        _ => "Couldn't move that window".into(),
    })
}

/// The outer rectangle that puts the visible frame at `wanted`, since `SetWindowPos` counts the invisible borders.
fn outer_target(wanted: Rect, outer: Rect, visible: Rect) -> Rect {
    let left = visible.left() - outer.left();
    let top = visible.top() - outer.top();
    let right = outer.right() - visible.right();
    let bottom = outer.bottom() - visible.bottom();
    Rect::from_min_max(wanted.min - vec2(left, top), wanted.max + vec2(right, bottom))
}

/// Monitors are known by their top left corner, which winit and Win32 agree on.
fn key_of(x: i32, y: i32) -> u64 {
    ((x as u32 as u64) << 32) | (y as u32 as u64)
}

unsafe extern "system" fn visit_monitor(monitor: HMONITOR, _: HDC, _: *mut RECT, found: LPARAM) -> BOOL {
    let mut info: MONITORINFO = unsafe { std::mem::zeroed() };
    info.cbSize = size_of::<MONITORINFO>() as u32;
    if unsafe { GetMonitorInfoW(monitor, &mut info) } != 0 {
        let found = unsafe { &mut *(found as *mut Vec<Screen>) };
        found.push(Screen {
            key: key_of(info.rcMonitor.left, info.rcMonitor.top),
            frame: rect_of(info.rcMonitor),
            work: rect_of(info.rcWork),
            scale: scale_of(monitor),
        });
    }
    TRUE
}

unsafe extern "system" fn visit_window(hwnd: HWND, found: LPARAM) -> BOOL {
    unsafe { &mut *(found as *mut Vec<HWND>) }.push(hwnd);
    TRUE
}

/// The monitor's effective DPI over 96.
fn scale_of(monitor: HMONITOR) -> f32 {
    let (mut x, mut y) = (0u32, 0u32);
    let read = unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut x, &mut y) };
    if read == 0 && x > 0 { x as f32 / BASE_DPI } else { 1.0 }
}

fn rect_of(rect: RECT) -> Rect {
    Rect::from_min_max(pos2(rect.left as f32, rect.top as f32), pos2(rect.right as f32, rect.bottom as f32))
}

fn window_rect(hwnd: HWND) -> Option<Rect> {
    let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    (unsafe { GetWindowRect(hwnd, &mut rect) } != 0).then(|| rect_of(rect))
}

fn visible_frame(hwnd: HWND) -> Option<Rect> {
    let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    let read = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS as u32,
            (&raw mut rect).cast(),
            size_of::<RECT>() as u32,
        )
    };
    if read == 0 { Some(rect_of(rect)) } else { window_rect(hwnd) }
}

fn grab_of(hwnd: HWND) -> Option<Grab> {
    let frame = visible_frame(hwnd)?;
    Some(Grab { handle: Handle { id: hwnd as u64, pid: process_of(hwnd) }, frame })
}

fn process_of(hwnd: HWND) -> u32 {
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    pid
}

/// A visible window of another app that isn't minimized, cloaked, a tool window or part of the shell.
fn is_candidate(hwnd: HWND) -> bool {
    if hwnd.is_null() || unsafe { IsWindowVisible(hwnd) } == 0 || unsafe { IsIconic(hwnd) } != 0 {
        return false;
    }
    let ex_style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32;
    if ex_style & WS_EX_TOOLWINDOW != 0 || hwnd == unsafe { GetShellWindow() } || is_cloaked(hwnd) {
        return false;
    }
    process_of(hwnd) != std::process::id() && !SHELL_CLASSES.contains(&class_of(hwnd).as_str())
}

/// Only windows with a title bar or a sizing border get tiled.
fn is_arrangeable(hwnd: HWND) -> bool {
    let style = unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) } as u32;
    style & (WS_CAPTION | WS_THICKFRAME) != 0
}

/// Cloaked windows exist but aren't shown, like those on another virtual desktop.
fn is_cloaked(hwnd: HWND) -> bool {
    let mut cloaked = 0u32;
    let read = unsafe {
        DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED as u32, (&raw mut cloaked).cast(), size_of::<u32>() as u32)
    };
    read == 0 && cloaked != 0
}

fn class_of(hwnd: HWND) -> String {
    let mut name = [0u16; 64];
    let len = unsafe { GetClassNameW(hwnd, name.as_mut_ptr(), name.len() as i32) };
    String::from_utf16_lossy(&name[..len.max(0) as usize])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invisible_borders_wrap_the_frame() {
        let outer = Rect::from_min_max(pos2(92.0, 100.0), pos2(1108.0, 808.0));
        let visible = Rect::from_min_max(pos2(100.0, 100.0), pos2(1100.0, 800.0));
        let wanted = Rect::from_min_max(pos2(0.0, 0.0), pos2(500.0, 400.0));
        let target = outer_target(wanted, outer, visible);
        assert_eq!(target, Rect::from_min_max(pos2(-8.0, 0.0), pos2(508.0, 408.0)));
    }

    #[test]
    fn monitor_keys_handle_negative_origins() {
        assert_ne!(key_of(-1920, 0), key_of(1920, 0));
        assert_eq!(key_of(0, 0), 0);
    }
}
