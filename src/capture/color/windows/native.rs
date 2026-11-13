//! The Win32 calls the loupe window needs: its tool window styles, hiding it from captures, keeping it on top,
//! and showing it without taking focus.

use std::ptr::null_mut;

use raw_window_handle::{HasWindowHandle as _, RawWindowHandle};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetWindowLongPtrW, HWND_TOPMOST, LWA_ALPHA, SW_HIDE, SW_SHOWNOACTIVATE, SWP_FRAMECHANGED,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetLayeredWindowAttributes, SetWindowDisplayAffinity,
    SetWindowLongPtrW, SetWindowPos, ShowWindow, WDA_EXCLUDEFROMCAPTURE, WS_EX_APPWINDOW, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};
use winit::window::Window;

#[derive(Clone, Copy)]
pub(super) struct Hwnd(HWND);

impl Hwnd {
    pub(super) fn of(window: &Window) -> Option<Self> {
        match window.window_handle().ok()?.as_raw() {
            RawWindowHandle::Win32(h) => Some(Self(h.hwnd.get() as HWND)),
            _ => None,
        }
    }

    /// Keeps the window out of the taskbar and Alt-Tab, stops it taking focus, and makes it layered.
    pub(super) fn make_passive_tool_window(self) {
        let old = unsafe { GetWindowLongPtrW(self.0, GWL_EXSTYLE) } as u32;
        let new = (old | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED) & !WS_EX_APPWINDOW;
        if new != old {
            let flags = SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED;
            unsafe {
                SetWindowLongPtrW(self.0, GWL_EXSTYLE, new as isize);
                SetWindowPos(self.0, null_mut(), 0, 0, 0, 0, flags);
            }
        }
        if old & WS_EX_LAYERED == 0 {
            unsafe { SetLayeredWindowAttributes(self.0, 0, 255, LWA_ALPHA) };
        }
    }

    /// Hides the window from screen captures, our own grab included. False on Windows versions that can't.
    pub(super) fn exclude_from_capture(self) -> bool {
        unsafe { SetWindowDisplayAffinity(self.0, WDA_EXCLUDEFROMCAPTURE) != 0 }
    }

    pub(super) fn raise(self) {
        unsafe { SetWindowPos(self.0, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE) };
    }

    pub(super) fn show_without_activating(self) {
        unsafe { ShowWindow(self.0, SW_SHOWNOACTIVATE) };
    }

    pub(super) fn hide(self) {
        unsafe { ShowWindow(self.0, SW_HIDE) };
    }
}

#[cfg(test)]
mod tests {
    use std::ptr::null;
    use std::time::{Duration, Instant};

    use windows_sys::Win32::Graphics::Gdi::{CreateSolidBrush, DeleteObject, UpdateWindow};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, RegisterClassW,
        UnregisterClassW, WDA_NONE, WNDCLASSW, WS_EX_TOPMOST, WS_POPUP,
    };

    use super::super::grabber::Grabber;
    use super::*;
    use crate::capture::color::loupe::GRAB;

    const MAGENTA: [u8; 3] = [255, 0, 255];

    fn pump(ms: u64) {
        let until = Instant::now() + Duration::from_millis(ms);
        while Instant::now() < until {
            let mut msg = MSG::default();
            while unsafe { PeekMessageW(&mut msg, null_mut(), 0, 0, PM_REMOVE) } != 0 {
                unsafe { DispatchMessageW(&msg) };
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    #[ignore = "briefly shows a small window on screen"]
    fn excluded_window_is_not_grabbed() {
        let (x, y, side) = (64, 64, 48);
        let middle = [x + side / 2, y + side / 2];
        let class: Vec<u16> = "notchbase sampler test\0".encode_utf16().collect();
        let instance = unsafe { GetModuleHandleW(null()) };
        let brush = unsafe { CreateSolidBrush(0x00FF_00FF) };
        let wc = WNDCLASSW {
            lpfnWndProc: Some(DefWindowProcW),
            hInstance: instance,
            hbrBackground: brush,
            lpszClassName: class.as_ptr(),
            ..Default::default()
        };
        assert_ne!(unsafe { RegisterClassW(&wc) }, 0);
        let raw = unsafe {
            CreateWindowExW(
                WS_EX_TOPMOST,
                class.as_ptr(),
                null(),
                WS_POPUP,
                x,
                y,
                side,
                side,
                null_mut(),
                null_mut(),
                instance,
                null(),
            )
        };
        assert!(!raw.is_null());
        let hwnd = Hwnd(raw);
        hwnd.make_passive_tool_window();
        unsafe { SetWindowDisplayAffinity(raw, WDA_NONE) };
        hwnd.show_without_activating();
        unsafe { UpdateWindow(raw) };
        pump(400);

        let grabber = Grabber::new().unwrap();
        let middle_px = (GRAB / 2) as i64;
        let seen = grabber.grab(middle).unwrap().pixel(middle_px, middle_px);
        let excluded = hwnd.exclude_from_capture();
        pump(400);
        let behind = grabber.grab(middle).unwrap().pixel(middle_px, middle_px);

        unsafe {
            DestroyWindow(raw);
            UnregisterClassW(class.as_ptr(), instance);
            DeleteObject(brush);
        }

        eprintln!("visible: {seen:?}, excluded ({excluded}): {behind:?}");
        assert_eq!(seen, MAGENTA, "the test window should show up in a grab before exclusion");
        assert!(excluded, "SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE) failed");
        assert_ne!(behind, MAGENTA, "the excluded window still shows up in the grab");
    }
}
