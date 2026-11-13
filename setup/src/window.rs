//! Window tweaks the frameless installer needs.

/// Turns off the frame and shadow Windows draws, and clips the 1 px edge it still leaves at the top.
#[cfg(windows)]
pub fn flatten(frame: &eframe::Frame, size: [i32; 2]) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::Graphics::Dwm::{DWMNCRP_DISABLED, DWMWA_NCRENDERING_POLICY, DwmSetWindowAttribute};
    use windows_sys::Win32::Graphics::Gdi::{CreateRectRgn, SetWindowRgn};

    const FRAME_EDGE: i32 = 1;

    let Ok(handle) = frame.window_handle() else { return };
    let RawWindowHandle::Win32(win32) = handle.as_raw() else { return };
    let hwnd = win32.hwnd.get() as _;
    let policy = DWMNCRP_DISABLED;
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_NCRENDERING_POLICY as u32,
            (&raw const policy).cast(),
            size_of_val(&policy) as u32,
        );
        let region = CreateRectRgn(0, FRAME_EDGE, size[0], size[1] + FRAME_EDGE);
        SetWindowRgn(hwnd, region, 1);
    }
}

#[cfg(not(windows))]
pub fn flatten(_frame: &eframe::Frame, _size: [i32; 2]) {}
