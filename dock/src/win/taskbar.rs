//! The Windows taskbar, put away while the dock runs, the way the macOS side hides the system Dock:
//!
//! 1. The taskbar's auto-hide setting and Windows' minimize animation are saved to `saved-taskbar.json`.
//! 2. The taskbar is set to auto-hide, so it gives its space back to apps, and its windows (one per screen) are
//!    hidden outright, so it never slides up under the dock. Explorer restarting brings it back, so it's hidden
//!    again every couple of seconds.
//! 3. While the dock draws minimizing itself, Windows' own minimize animation is off.
//!
//! Everything comes back as it was when the dock quits, a normal quit, `--quit`, a closed console or a panic. A dock
//! that was killed outright leaves the file behind, and the next start (or `--restore-taskbar`) puts it back first.
//!
//! The dock also keeps the strip it stands on as an app bar, so maximized windows end above it.

use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use windows_sys::Win32::Foundation::{HWND, LPARAM, RECT, TRUE};
use windows_sys::Win32::UI::Shell::{
    ABE_BOTTOM, ABM_GETSTATE, ABM_NEW, ABM_QUERYPOS, ABM_REMOVE, ABM_SETPOS, ABM_SETSTATE, ABS_ALWAYSONTOP,
    ABS_AUTOHIDE, APPBARDATA, SHAppBarMessage,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    ANIMATIONINFO, EnumWindows, FindWindowW, IsWindowVisible, SPI_GETANIMATION, SPI_SETANIMATION, SPIF_SENDCHANGE,
    SPIF_UPDATEINIFILE, SW_HIDE, SW_SHOWNA, ShowWindow, SystemParametersInfoW,
};
use windows_sys::core::BOOL;

use super::sys;

const FILE: &str = "saved-taskbar.json";

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Saved {
    /// The taskbar was already set to auto-hide.
    pub autohide: bool,
    /// Windows animated minimizing and maximizing.
    pub min_animate: bool,
}

pub fn state_path(dir: &Path) -> PathBuf {
    dir.join(FILE)
}

/// How things are now, before the dock changes anything.
pub fn read_current() -> Saved {
    Saved { autohide: autohide(), min_animate: min_animate() }
}

/// Saves the settings (unless a save from a crashed dock is still waiting to be put back) and hides the taskbar.
pub fn hide(dir: &Path) -> io::Result<()> {
    let path = state_path(dir);
    if !path.exists() {
        std::fs::create_dir_all(dir)?;
        let text = serde_json::to_string_pretty(&read_current()).map_err(io::Error::other)?;
        let partial = path.with_extension("json.partial");
        std::fs::write(&partial, text)?;
        std::fs::rename(&partial, &path)?;
    }
    set_autohide(true);
    keep_hidden();
    Ok(())
}

/// Hides the taskbar's windows again if Explorer showed them (it restarted, or Start opened).
pub fn keep_hidden() {
    for hwnd in taskbars() {
        if unsafe { IsWindowVisible(hwnd) } != 0 {
            unsafe { ShowWindow(hwnd, SW_HIDE) };
        }
    }
}

/// Puts back what `hide` saved. True when there was something to put back.
pub fn restore(dir: &Path) -> io::Result<bool> {
    let path = state_path(dir);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(false);
    };
    let saved: Saved = serde_json::from_str(&text).unwrap_or(Saved { autohide: false, min_animate: true });
    show_taskbars();
    set_autohide(saved.autohide);
    set_min_animate(saved.min_animate);
    std::fs::remove_file(&path)?;
    Ok(true)
}

/// Shows the taskbar again even without a saved file, for `--restore-taskbar` after a lost file.
pub fn show_taskbars() {
    for hwnd in taskbars() {
        unsafe { ShowWindow(hwnd, SW_SHOWNA) };
    }
}

/// The taskbar on the main screen and the ones on the others.
fn taskbars() -> Vec<HWND> {
    unsafe extern "system" fn visit(hwnd: HWND, found: LPARAM) -> BOOL {
        if sys::class_name(hwnd) == "Shell_SecondaryTrayWnd" {
            unsafe { &mut *(found as *mut Vec<HWND>) }.push(hwnd);
        }
        TRUE
    }
    let mut found = Vec::new();
    let main = unsafe { FindWindowW(sys::wide("Shell_TrayWnd").as_ptr(), std::ptr::null()) };
    if !main.is_null() {
        found.push(main);
    }
    unsafe { EnumWindows(Some(visit), (&raw mut found) as LPARAM) };
    found
}

fn bar_data() -> APPBARDATA {
    let mut data: APPBARDATA = unsafe { std::mem::zeroed() };
    data.cbSize = size_of::<APPBARDATA>() as u32;
    data
}

fn autohide() -> bool {
    let mut data = bar_data();
    let state = unsafe { SHAppBarMessage(ABM_GETSTATE, &mut data) } as u32;
    state & ABS_AUTOHIDE != 0
}

fn set_autohide(on: bool) {
    let mut data = bar_data();
    data.hWnd = unsafe { FindWindowW(sys::wide("Shell_TrayWnd").as_ptr(), std::ptr::null()) };
    data.lParam = if on { ABS_AUTOHIDE as LPARAM } else { ABS_ALWAYSONTOP as LPARAM };
    unsafe { SHAppBarMessage(ABM_SETSTATE, &mut data) };
}

fn min_animate() -> bool {
    let mut info = ANIMATIONINFO { cbSize: size_of::<ANIMATIONINFO>() as u32, iMinAnimate: 1 };
    unsafe { SystemParametersInfoW(SPI_GETANIMATION, info.cbSize, (&raw mut info).cast(), 0) };
    info.iMinAnimate != 0
}

/// Windows' own minimize and maximize animation, on or off for everyone. Off while the dock draws minimizing.
pub fn set_min_animate(on: bool) {
    if min_animate() == on {
        return;
    }
    let mut info = ANIMATIONINFO { cbSize: size_of::<ANIMATIONINFO>() as u32, iMinAnimate: i32::from(on) };
    unsafe {
        SystemParametersInfoW(
            SPI_SETANIMATION,
            info.cbSize,
            (&raw mut info).cast(),
            SPIF_UPDATEINIFILE | SPIF_SENDCHANGE,
        )
    };
}

/// The minimize animation as saved before the dock started, for the "System" minimize effect.
pub fn saved_min_animate(dir: &Path) -> bool {
    std::fs::read_to_string(state_path(dir))
        .ok()
        .and_then(|t| serde_json::from_str::<Saved>(&t).ok())
        .is_none_or(|s| s.min_animate)
}

/// The strip along a screen's bottom the dock keeps from maximized windows.
pub struct AppBar {
    hwnd: HWND,
    registered: bool,
    last: Option<RECT>,
}

impl AppBar {
    pub fn new(hwnd: HWND) -> Self {
        Self { hwnd, registered: false, last: None }
    }

    /// Keeps `height` pixels along the bottom of `screen` (physical pixels), or nothing when `height` is 0.
    pub fn reserve(&mut self, screen: RECT, height: i32) {
        if height <= 0 {
            self.release();
            return;
        }
        let want = RECT { left: screen.left, top: screen.bottom - height, right: screen.right, bottom: screen.bottom };
        if self.registered && self.last.is_some_and(|r| same(r, want)) {
            return;
        }
        let mut data = bar_data();
        data.hWnd = self.hwnd;
        if !self.registered {
            // The dock's windows never handle the app bar's callback message: it only asks again now and then.
            data.uCallbackMessage = 0;
            self.registered = unsafe { SHAppBarMessage(ABM_NEW, &mut data) } != 0;
            if !self.registered {
                return;
            }
        }
        data.uEdge = ABE_BOTTOM;
        data.rc = want;
        unsafe { SHAppBarMessage(ABM_QUERYPOS, &mut data) };
        data.rc.top = data.rc.bottom - height;
        unsafe { SHAppBarMessage(ABM_SETPOS, &mut data) };
        self.last = Some(want);
    }

    /// Asks again on the next `reserve`, after Explorer restarted or the screens changed.
    pub fn refresh(&mut self) {
        self.last = None;
    }

    pub fn release(&mut self) {
        if self.registered {
            let mut data = bar_data();
            data.hWnd = self.hwnd;
            unsafe { SHAppBarMessage(ABM_REMOVE, &mut data) };
            self.registered = false;
            self.last = None;
        }
    }
}

impl Drop for AppBar {
    fn drop(&mut self) {
        self.release();
    }
}

fn same(a: RECT, b: RECT) -> bool {
    (a.left, a.top, a.right, a.bottom) == (b.left, b.top, b.right, b.bottom)
}
