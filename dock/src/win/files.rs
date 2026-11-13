//! Files out of a stack: their pictures (the shell's thumbnails, else their icons) and real file drags to other
//! apps, the way the notch's shelf drags them on Windows.

use std::path::{Path, PathBuf};

use raw_window_handle::{HasWindowHandle as _, RawWindowHandle};
use winit::window::Window;

/// A picture of `path` at most `px` pixels on its longer side, and whether it shows the contents (rather than an
/// icon). Any thread.
pub fn thumbnail(path: &Path, px: u32) -> Option<(egui::ColorImage, bool)> {
    super::shell::thumbnail(path, px)
}

pub fn dragging() -> bool {
    super::shell::dragging()
}

pub fn take_ended() -> bool {
    super::shell::take_ended()
}

/// Drags `paths` out of the dock's window. Runs until the drop.
pub fn drag(window: &Window, paths: &[PathBuf]) -> Result<(), String> {
    let RawWindowHandle::Win32(handle) = window.window_handle().map_err(|e| e.to_string())?.as_raw() else {
        return Err("No window to drag from".into());
    };
    let hwnd = ::windows::Win32::Foundation::HWND(handle.hwnd.get() as *mut _);
    let result = super::shell::drag(hwnd, paths);
    window.request_redraw();
    result
}
