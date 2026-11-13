//! Everything the dock asks of Windows that macOS answers in `mac.rs`: running apps, icons, opening and quitting,
//! the Recycle Bin, the pointer, the screens and the frame of an app's front window. Positions are physical pixels
//! on the virtual screen; the event loop turns them into each dock's points.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use windows_sys::Win32::Foundation::{HWND, LPARAM, POINT, RECT, TRUE, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW};
use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess};
use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput, VK_LBUTTON, VK_MBUTTON,
    VK_MENU, VK_RBUTTON, VK_SHIFT,
};
use windows_sys::Win32::UI::Shell::{
    FO_DELETE, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI,
    SHEmptyRecycleBinW, SHFILEOPSTRUCTW, SHFileOperationW, SHQUERYRBINFO, SHQueryRecycleBinW, ShellExecuteW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, GetCursorPos, GetForegroundWindow, PostMessageW, SW_MINIMIZE, SW_RESTORE, SW_SHOWNORMAL,
    SetForegroundWindow, ShowWindow, WM_CLOSE,
};
use windows_sys::core::BOOL;

use super::sys;

/// What menus call the file manager and the Recycle Bin.
pub const SHOW_IN_FILES: &str = "Show in File Explorer";
pub const OPEN_IN_FILES: &str = "Open in File Explorer";
pub const MOVE_TO_TRASH: &str = "Delete";
pub const EMPTY_TRASH: &str = "Empty Recycle Bin";
pub const EMPTY_TRASH_CONFIRM: &str = "Click to Empty Recycle Bin for Good";
pub const TRASH_NAME: &str = "Recycle Bin";
/// The Recycle Bin as the shell names it, which the dock keeps as the Trash tile's path.
pub const RECYCLE_BIN: &str = "::{645FF040-5081-101B-9F08-00AA002F954E}";

#[derive(Clone, Debug, PartialEq)]
pub struct Running {
    pub pid: i32,
    pub path: Option<PathBuf>,
    pub name: String,
    pub active: bool,
    pub hidden: bool,
}

/// The order apps were first seen running, so running apps keep their places instead of following the z-order.
static FIRST_SEEN: Mutex<Vec<i32>> = Mutex::new(Vec::new());

/// Apps with a window the taskbar would show, in the order they were first seen, without this one. File Explorer
/// counts only while it has a folder window open.
pub fn running_apps() -> Vec<Running> {
    let me = std::process::id() as i32;
    let front = frontmost_pid();
    let mut out: Vec<Running> = Vec::new();
    for hwnd in sys::app_windows() {
        let pid = sys::pid_of(hwnd);
        if pid == me || pid <= 0 || out.iter().any(|r| r.pid == pid) {
            continue;
        }
        let Some(path) = sys::program_of(pid) else { continue };
        if is_explorer(&path) && sys::class_name(hwnd) != "CabinetWClass" {
            continue;
        }
        out.push(Running {
            pid,
            name: display_name(&path),
            path: Some(path),
            active: front == Some(pid),
            hidden: false,
        });
    }
    let mut seen = FIRST_SEEN.lock().unwrap_or_else(|e| e.into_inner());
    seen.retain(|pid| out.iter().any(|r| r.pid == *pid));
    for r in &out {
        if !seen.contains(&r.pid) {
            seen.push(r.pid);
        }
    }
    out.sort_by_key(|r| seen.iter().position(|p| *p == r.pid));
    out
}

fn is_explorer(path: &Path) -> bool {
    path.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("explorer.exe"))
}

/// The name an app goes by: its program's description ("Google Chrome"), else its file name. Folders and
/// shortcuts by their names.
pub fn display_name(path: &Path) -> String {
    static NAMES: Mutex<Option<HashMap<PathBuf, String>>> = Mutex::new(None);
    let mut names = NAMES.lock().unwrap_or_else(|e| e.into_inner());
    let names = names.get_or_insert_with(HashMap::new);
    if let Some(name) = names.get(path) {
        return name.clone();
    }
    let stem = || path.file_stem().or(path.file_name()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let name = if is_explorer(path) {
        "File Explorer".to_owned()
    } else if is_exe(path) {
        file_description(path).filter(|d| !d.trim().is_empty()).unwrap_or_else(stem)
    } else {
        stem()
    };
    names.insert(path.to_path_buf(), name.clone());
    name
}

fn is_exe(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe"))
}

/// A program's FileDescription from its version resource, in its own first language.
fn file_description(path: &Path) -> Option<String> {
    use windows_sys::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW};
    let file = sys::wide_path(path);
    let size = unsafe { GetFileVersionInfoSizeW(file.as_ptr(), std::ptr::null_mut()) };
    if size == 0 {
        return None;
    }
    let mut data = vec![0u8; size as usize];
    if unsafe { GetFileVersionInfoW(file.as_ptr(), 0, size, data.as_mut_ptr().cast()) } == 0 {
        return None;
    }
    let mut pointer: *mut std::ffi::c_void = std::ptr::null_mut();
    let mut len = 0u32;
    let query = sys::wide("\\VarFileInfo\\Translation");
    let languages =
        if unsafe { VerQueryValueW(data.as_ptr().cast(), query.as_ptr(), &mut pointer, &mut len) } != 0 && len >= 4 {
            let pair = unsafe { std::slice::from_raw_parts(pointer as *const u16, 2) };
            vec![(pair[0], pair[1]), (0x0409, 0x04B0), (0x0409, 0x04E4)]
        } else {
            vec![(0x0409, 0x04B0), (0x0409, 0x04E4)]
        };
    for (language, codepage) in languages {
        let key = sys::wide(&format!("\\StringFileInfo\\{language:04x}{codepage:04x}\\FileDescription"));
        if unsafe { VerQueryValueW(data.as_ptr().cast(), key.as_ptr(), &mut pointer, &mut len) } != 0 && len > 0 {
            let text = unsafe { std::slice::from_raw_parts(pointer as *const u16, len as usize) };
            let end = text.iter().position(|c| *c == 0).unwrap_or(text.len());
            return Some(String::from_utf16_lossy(&text[..end]).trim().to_owned());
        }
    }
    None
}

/// A file's icon from the shell, `px` pixels square.
pub fn icon_for_path(path: &Path, px: u32) -> Option<egui::ColorImage> {
    if path.as_os_str() == RECYCLE_BIN {
        return trash_icon(false, px);
    }
    super::shell::icon(path, art(px)).map(|image| square(image, px))
}

/// How big the art is inside an icon's canvas. Windows icons fill their square, where macOS app icons keep a margin
/// the dock's layout counts on, so they're drawn with the same margin.
fn art(px: u32) -> u32 {
    ((px as f32) * 0.84).round().max(1.0) as u32
}

/// A running app's icon: its program's.
pub fn icon_for_pid(pid: i32, px: u32) -> Option<egui::ColorImage> {
    icon_for_path(&sys::program_of(pid)?, px)
}

pub fn icon_from_file(path: &Path, px: u32) -> Option<egui::ColorImage> {
    icon_for_path(path, px)
}

/// The Recycle Bin's own icon. The shell already draws it full or empty as it is.
pub fn trash_icon(_full: bool, px: u32) -> Option<egui::ColorImage> {
    let name = ::windows::core::HSTRING::from(format!("shell:{RECYCLE_BIN}"));
    super::shell::icon_named(&name, art(px)).map(|image| square(image, px))
}

/// Centres an icon in a square canvas, as the dock lays icons out.
fn square(image: egui::ColorImage, px: u32) -> egui::ColorImage {
    let [w, h] = image.size;
    let side = (px as usize).max(w).max(h);
    if w == side && h == side {
        return image;
    }
    let mut out = egui::ColorImage::new([side, side], vec![egui::Color32::TRANSPARENT; side * side]);
    let (x0, y0) = ((side - w) / 2, (side - h) / 2);
    for y in 0..h {
        for x in 0..w {
            out.pixels[(y + y0) * side + x + x0] = image.pixels[y * w + x];
        }
    }
    out
}

/// Opens a file, a folder or an app the way double-clicking it in File Explorer would. A running app opens another
/// window, as the Dock does for an app with none.
pub fn open(path: &Path) {
    if path.as_os_str() == RECYCLE_BIN {
        shell_execute(Path::new("explorer.exe"), &format!("shell:{RECYCLE_BIN}"));
        return;
    }
    shell_execute(path, "");
}

pub fn open_with(app: &Path, files: &[PathBuf]) {
    if is_explorer(app) {
        for file in files {
            open(file);
        }
        return;
    }
    let args: Vec<String> = files.iter().map(|f| format!("\"{}\"", f.display())).collect();
    shell_execute(app, &args.join(" "));
}

fn shell_execute(file: &Path, args: &str) {
    let file = sys::wide_path(file);
    let verb = sys::wide("open");
    let args = sys::wide(args);
    unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            if args.len() > 1 { args.as_ptr() } else { std::ptr::null() },
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
}

pub fn reveal(path: &Path) {
    super::shell::reveal(path);
}

/// The app's windows, front to back.
pub fn windows_of(pid: i32) -> Vec<HWND> {
    sys::app_windows().into_iter().filter(|h| sys::pid_of(*h) == pid).collect()
}

/// Asks each of the app's windows to close, the way its own close button would.
pub fn quit(pid: i32) {
    for hwnd in windows_of(pid) {
        unsafe { PostMessageW(hwnd, WM_CLOSE, 0 as WPARAM, 0 as LPARAM) };
    }
}

/// Brings an app's front window forward, restoring it if it's minimized.
pub fn activate(pid: i32) {
    if let Some(hwnd) = windows_of(pid).into_iter().next() {
        bring_forward(hwnd);
    }
}

/// Puts a window in front. Windows lets a process take the foreground only right after input; the dock just had
/// that click, and a tap of Alt covers the rest.
pub fn bring_forward(hwnd: HWND) {
    unsafe {
        if sys::iconic(hwnd) {
            ShowWindow(hwnd, SW_RESTORE);
        }
        if SetForegroundWindow(hwnd) == 0 {
            tap_alt();
            SetForegroundWindow(hwnd);
        }
        BringWindowToTop(hwnd);
    }
}

fn tap_alt() {
    let key = |flags| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VK_MENU, wScan: 0, dwFlags: flags, time: 0, dwExtraInfo: 0 } },
    };
    let inputs = [key(0), key(KEYEVENTF_KEYUP)];
    unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), size_of::<INPUT>() as i32) };
}

/// Ends an app without asking, for apps that have stopped answering.
pub fn force_quit(pid: i32) {
    unsafe {
        let process = OpenProcess(PROCESS_TERMINATE, 0, pid as u32);
        if !process.is_null() {
            TerminateProcess(process, 1);
            windows_sys::Win32::Foundation::CloseHandle(process);
        }
    }
}

fn held(key: u16) -> bool {
    (unsafe { GetAsyncKeyState(i32::from(key)) }) < 0
}

/// Whether the key that turns Quit into Force Quit is held: Shift, on Windows.
pub fn option_held() -> bool {
    held(VK_SHIFT)
}

/// Show All Windows: every one of the app's windows comes forward, restored.
pub fn app_expose(pid: i32) {
    let windows = windows_of(pid);
    for hwnd in windows.iter().rev() {
        bring_forward(*hwnd);
    }
}

/// Hide: the app's windows go down into the dock.
pub fn hide(pid: i32) {
    for hwnd in windows_of(pid) {
        unsafe { ShowWindow(hwnd, SW_MINIMIZE) };
    }
}

pub fn trash_dir() -> Option<PathBuf> {
    Some(PathBuf::from(RECYCLE_BIN))
}

/// Sends a file to the Recycle Bin, quietly.
pub fn move_to_trash(path: &Path) -> bool {
    let mut from = sys::wide_path(path);
    from.push(0);
    let mut op: SHFILEOPSTRUCTW = unsafe { std::mem::zeroed() };
    op.wFunc = FO_DELETE;
    op.pFrom = from.as_ptr();
    op.fFlags = (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_SILENT | FOF_NOERRORUI) as u16;
    unsafe { SHFileOperationW(&mut op) == 0 && op.fAnyOperationsAborted == 0 }
}

pub fn empty_trash() {
    std::thread::spawn(|| unsafe {
        SHEmptyRecycleBinW(std::ptr::null_mut(), std::ptr::null(), SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI)
    });
}

pub fn trash_is_full() -> bool {
    let mut info: SHQUERYRBINFO = unsafe { std::mem::zeroed() };
    info.cbSize = size_of::<SHQUERYRBINFO>() as u32;
    unsafe { SHQueryRecycleBinW(std::ptr::null(), &mut info) == 0 && info.i64NumItems > 0 }
}

/// Whether apps are set to dark (Settings › Personalization › Colors).
pub fn dark_mode() -> bool {
    let key = sys::wide("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
    let value = sys::wide("AppsUseLightTheme");
    let mut light: u32 = 1;
    let mut size = size_of::<u32>() as u32;
    let got = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&raw mut light).cast(),
            &mut size,
        )
    };
    got == 0 && light == 0
}

/// The pointer in physical pixels on the virtual screen.
pub fn pointer() -> egui::Pos2 {
    let mut at = POINT { x: 0, y: 0 };
    unsafe { GetCursorPos(&mut at) };
    egui::pos2(at.x as f32, at.y as f32)
}

pub fn mouse_buttons_held() -> bool {
    held(VK_LBUTTON) || held(VK_RBUTTON) || held(VK_MBUTTON)
}

/// A screen: its whole area and the part apps get, in physical pixels, and its scale.
#[derive(Clone, Debug, PartialEq)]
pub struct Screen {
    /// Stays the same while the screen is plugged in: a hash of its device name.
    pub id: u32,
    pub frame: RectPx,
    pub work: RectPx,
    pub scale: f32,
    pub primary: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RectPx {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl RectPx {
    pub fn from(r: RECT) -> Self {
        Self { left: r.left, top: r.top, right: r.right, bottom: r.bottom }
    }
    pub fn width(&self) -> i32 {
        self.right - self.left
    }
    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
    pub fn contains(&self, p: egui::Pos2) -> bool {
        p.x >= self.left as f32 && p.x < self.right as f32 && p.y >= self.top as f32 && p.y < self.bottom as f32
    }
    pub fn rect(&self) -> egui::Rect {
        egui::Rect::from_min_max(
            egui::pos2(self.left as f32, self.top as f32),
            egui::pos2(self.right as f32, self.bottom as f32),
        )
    }
}

/// Every screen, the primary one first.
pub fn screens() -> Vec<Screen> {
    unsafe extern "system" fn visit(monitor: HMONITOR, _: HDC, _: *mut RECT, found: LPARAM) -> BOOL {
        let found = unsafe { &mut *(found as *mut Vec<Screen>) };
        let mut info: MONITORINFOEXW = unsafe { std::mem::zeroed() };
        info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
        if unsafe { GetMonitorInfoW(monitor, (&raw mut info).cast()) } != 0 {
            let (mut dpi_x, mut dpi_y) = (96u32, 96u32);
            unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) };
            let end = info.szDevice.iter().position(|c| *c == 0).unwrap_or(info.szDevice.len());
            let name = String::from_utf16_lossy(&info.szDevice[..end]);
            found.push(Screen {
                id: hash(&name),
                frame: RectPx::from(info.monitorInfo.rcMonitor),
                work: RectPx::from(info.monitorInfo.rcWork),
                scale: (dpi_x.max(48) as f32) / 96.0,
                // MONITORINFOF_PRIMARY
                primary: info.monitorInfo.dwFlags & 1 != 0,
            });
        }
        TRUE
    }
    let mut found: Vec<Screen> = Vec::new();
    unsafe { EnumDisplayMonitors(std::ptr::null_mut(), std::ptr::null(), Some(visit), (&raw mut found) as LPARAM) };
    found.sort_by_key(|s| (!s.primary, s.frame.left, s.frame.top));
    found
}

fn hash(text: &str) -> u32 {
    text.bytes().fold(2_166_136_261u32, |h, b| (h ^ u32::from(b)).wrapping_mul(16_777_619))
}

pub fn frontmost_pid() -> Option<i32> {
    let hwnd = unsafe { GetForegroundWindow() };
    (!hwnd.is_null()).then(|| sys::pid_of(hwnd)).filter(|p| *p > 0)
}

/// The frame of the frontmost window `pid` shows, in physical pixels.
pub fn front_window_frame(pid: i32) -> Option<egui::Rect> {
    windows_of(pid)
        .into_iter()
        .filter(|h| !sys::iconic(*h))
        .find_map(sys::frame)
        .map(sys::rect_of)
        .filter(|r| r.width() > 80.0 && r.height() > 60.0)
}

/// Whether a file is something the dock can keep as an app: a program, or a shortcut to one.
pub fn is_app(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe") || e.eq_ignore_ascii_case("lnk"))
}

/// The user's folder.
pub fn home() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from)
}
