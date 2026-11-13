//! Dragging files across the edge of the screen on Windows. A drag between other apps can't be
//! read from outside, so a small, almost invisible window is put under the cursor while it's
//! held against the edge: the drag passes over it and hands over its files (as they're dropped
//! on the other side instead, a drop on this window itself does nothing). Files coming the other
//! way are picked up from the same window: it goes under the cursor, the left button goes down
//! on it, and it starts the shell's own drag, which drops where the button is let go.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use windows::Win32::Foundation::{HWND, POINTL};
use windows::Win32::System::Com::{DVASPECT_CONTENT, FORMATETC, IDataObject, TYMED_HGLOBAL};
use windows::Win32::System::Ole::{
    CF_HDROP, DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_NONE, IDropTarget, IDropTarget_Impl, OleInitialize,
    RegisterDragDrop, ReleaseStgMedium,
};
use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    BHID_DataObject, DragQueryFileW, HDROP, ILFree, SHCreateShellItemArrayFromIDLists, SHDoDragDrop, SHParseDisplayName,
};
use windows::core::{HSTRING, Ref, implement};
use windows_sys::Win32::Foundation::{LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, MOUSEEVENTF_LEFTDOWN,
    MOUSEINPUT, SendInput,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetCursorPos, GetMessageW, LWA_ALPHA, MA_NOACTIVATE, MSG,
    PostMessageW, RegisterClassW, SW_HIDE, SWP_NOACTIVATE, SWP_SHOWWINDOW, SetCursorPos, SetLayeredWindowAttributes,
    SetWindowPos, ShowWindow, TranslateMessage, WM_APP, WM_LBUTTONDOWN, WM_MOUSEACTIVATE, WNDCLASSW, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

use super::windows::MARK;

const SIZE: i32 = 48;
const HWND_TOPMOST: windows_sys::Win32::Foundation::HWND = -1isize as _;
/// Put the window under the cursor at (wparam, lparam), to catch a drag passing over.
const PROBE: u32 = WM_APP + 10;
const HIDE: u32 = WM_APP + 11;
/// Put the window under the cursor and press the left button on it, to pick files up.
const PICK: u32 = WM_APP + 12;
const ESCAPE_SCAN: u16 = 0x01;

#[derive(Default)]
struct State {
    window: AtomicUsize,
    /// What a drag passing over the window carries.
    files: Mutex<Option<Vec<PathBuf>>>,
    /// Files to pick up when the button goes down on the window.
    pending: Mutex<Option<Vec<PathBuf>>>,
    probing: AtomicBool,
}

fn state() -> &'static State {
    static STATE: OnceLock<State> = OnceLock::new();
    STATE.get_or_init(State::default)
}

/// The window, on a thread of its own that pumps its messages, made the first time it's needed.
fn window() -> Option<windows_sys::Win32::Foundation::HWND> {
    static STARTED: OnceLock<bool> = OnceLock::new();
    let started = *STARTED.get_or_init(|| {
        let (ready, made) = std::sync::mpsc::channel();
        let spawned = std::thread::Builder::new().name("roam-drag".into()).spawn(move || {
            // SAFETY: OLE, the window and its drop target live on this thread, which pumps
            // their messages for as long as the program runs.
            unsafe {
                let _ = OleInitialize(None);
                let window = make_window();
                if window.is_null() {
                    let _ = ready.send(false);
                    return;
                }
                let target: IDropTarget = Probe.into();
                let registered = RegisterDragDrop(HWND(window), &target).is_ok();
                state().window.store(window as usize, Ordering::Release);
                let _ = ready.send(registered);
                let mut message: MSG = std::mem::zeroed();
                while GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) > 0 {
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
        });
        spawned.is_ok() && made.recv().unwrap_or(false)
    });
    let window = state().window.load(Ordering::Acquire) as windows_sys::Win32::Foundation::HWND;
    (started && !window.is_null()).then_some(window)
}

fn post(message: u32, wparam: WPARAM, lparam: LPARAM) -> bool {
    let Some(window) = window() else { return false };
    // SAFETY: posting to a window this program owns.
    unsafe { PostMessageW(window, message, wparam, lparam) != 0 }
}

/// The left button went down or up here.
pub fn left_button(down: bool) {
    *state().files.lock().unwrap_or_else(|e| e.into_inner()) = None;
    if !down && state().probing.swap(false, Ordering::Relaxed) {
        post(HIDE, 0, 0);
    }
}

/// The files being dragged over the edge at `at`, once the drag has passed over the window.
pub fn dragged_files(at: [f64; 2]) -> Option<Vec<PathBuf>> {
    if !state().probing.swap(true, Ordering::Relaxed) {
        let (x, y) = (at[0].round() as i32, at[1].round() as i32);
        post(PROBE, x as isize as usize, y as isize);
    }
    state().files.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Escape ends the drag where it started, as it would by hand.
pub fn cancel_drag() {
    let key = |up: bool| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: 0,
                wScan: ESCAPE_SCAN,
                dwFlags: KEYEVENTF_SCANCODE | if up { KEYEVENTF_KEYUP } else { 0 },
                time: 0,
                dwExtraInfo: MARK,
            },
        },
    };
    let inputs = [key(false), key(true)];
    // SAFETY: valid inputs, with their size.
    unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32) };
    if state().probing.swap(false, Ordering::Relaxed) {
        post(HIDE, 0, 0);
    }
}

/// Picks `paths` up under the cursor.
pub fn drag_in(paths: Vec<PathBuf>) -> bool {
    if window().is_none() {
        return false;
    }
    *state().pending.lock().unwrap_or_else(|e| e.into_inner()) = Some(paths);
    state().probing.store(false, Ordering::Relaxed);
    post(PICK, 0, 0)
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

unsafe fn make_window() -> windows_sys::Win32::Foundation::HWND {
    let class = wide("RoamDrag");
    // SAFETY: a window class with a valid procedure and name, and a tiny, nearly invisible tool
    // window of it that never takes the focus.
    unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let mut info: WNDCLASSW = std::mem::zeroed();
        info.lpfnWndProc = Some(window_proc);
        info.hInstance = instance;
        info.lpszClassName = class.as_ptr();
        RegisterClassW(&info);
        let window = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED | WS_EX_NOACTIVATE,
            class.as_ptr(),
            class.as_ptr(),
            WS_POPUP,
            0,
            0,
            SIZE,
            SIZE,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        );
        if !window.is_null() {
            SetLayeredWindowAttributes(window, 0, 1, LWA_ALPHA);
        }
        window
    }
}

/// Shows the window centred on (x, y), above everything.
unsafe fn place(window: windows_sys::Win32::Foundation::HWND, x: i32, y: i32) {
    // SAFETY: this thread owns the window.
    unsafe {
        SetWindowPos(window, HWND_TOPMOST, x - SIZE / 2, y - SIZE / 2, SIZE, SIZE, SWP_NOACTIVATE | SWP_SHOWWINDOW);
    }
}

unsafe extern "system" fn window_proc(
    window: windows_sys::Win32::Foundation::HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // SAFETY: every call below acts on this thread's own window, or the cursor.
    unsafe {
        match message {
            PROBE => {
                let (x, y) = (wparam as isize as i32, lparam as i32);
                place(window, x, y);
                // A drag only notices the window when the cursor moves over it: a step inwards
                // and back.
                let (inward_x, inward_y) = inward(x, y);
                SetCursorPos(inward_x, inward_y);
                SetCursorPos(x, y);
                0
            }
            HIDE => {
                ShowWindow(window, SW_HIDE);
                0
            }
            PICK => {
                let mut at = POINT { x: 0, y: 0 };
                GetCursorPos(&mut at);
                place(window, at.x, at.y);
                let input = INPUT {
                    r#type: INPUT_MOUSE,
                    Anonymous: INPUT_0 {
                        mi: MOUSEINPUT {
                            dx: 0,
                            dy: 0,
                            mouseData: 0,
                            dwFlags: MOUSEEVENTF_LEFTDOWN,
                            time: 0,
                            dwExtraInfo: MARK,
                        },
                    },
                };
                SendInput(1, &input, std::mem::size_of::<INPUT>() as i32);
                0
            }
            WM_LBUTTONDOWN => {
                let pending = state().pending.lock().unwrap_or_else(|e| e.into_inner()).take();
                ShowWindow(window, SW_HIDE);
                if let Some(paths) = pending {
                    // Runs the drag until the button is let go, pumping messages meanwhile.
                    let _ = start(&paths);
                }
                0
            }
            WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
            _ => DefWindowProcW(window, message, wparam, lparam),
        }
    }
}

/// One step from (x, y) towards the middle of its screen.
fn inward(x: i32, y: i32) -> (i32, i32) {
    // SAFETY: plain monitor lookups with a properly sized struct.
    unsafe {
        let monitor = MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST);
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(monitor, &mut info) == 0 {
            return (x, y + 1);
        }
        let r = info.rcMonitor;
        let mid = ((r.left + r.right) / 2, (r.top + r.bottom) / 2);
        (x + (mid.0 - x).signum(), y + (mid.1 - y).signum())
    }
}

/// The shell's own drag of `paths`, as Explorer would start it.
fn start(paths: &[PathBuf]) -> windows::core::Result<()> {
    let lists: Vec<ItemIdList> = paths.iter().filter_map(|path| ItemIdList::for_path(path)).collect();
    if lists.is_empty() {
        return Ok(());
    }
    let pointers: Vec<*const ITEMIDLIST> = lists.iter().map(|list| list.0.cast_const()).collect();
    // SAFETY: the ID lists live until the drag is over.
    unsafe {
        let items = SHCreateShellItemArrayFromIDLists(&pointers)?;
        let data: IDataObject = items.BindToHandler(None, &BHID_DataObject)?;
        SHDoDragDrop(None, &data, None, DROPEFFECT_COPY)?;
    }
    Ok(())
}

struct ItemIdList(*mut ITEMIDLIST);

impl ItemIdList {
    fn for_path(path: &Path) -> Option<Self> {
        let mut list = std::ptr::null_mut();
        let name = HSTRING::from(plain(path).as_path());
        // SAFETY: parses a path into an ID list this owns.
        unsafe { SHParseDisplayName(&name, None, &mut list, 0, None) }.ok()?;
        Some(Self(list))
    }
}

impl Drop for ItemIdList {
    fn drop(&mut self) {
        // SAFETY: the list came from SHParseDisplayName.
        unsafe { ILFree(Some(self.0)) };
    }
}

/// A `\\?\C:\…` path as the shell reads it.
fn plain(path: &Path) -> PathBuf {
    match path.to_str().and_then(|text| text.strip_prefix(r"\\?\")) {
        Some(rest) if !rest.starts_with("UNC") => PathBuf::from(rest),
        _ => path.to_path_buf(),
    }
}

/// The window, as a place for a drag to pass over.
#[implement(IDropTarget)]
struct Probe;

impl IDropTarget_Impl for Probe_Impl {
    fn DragEnter(
        &self,
        data: Ref<IDataObject>,
        _keys: MODIFIERKEYS_FLAGS,
        _point: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        let files = data.as_ref().map(dropped_files).unwrap_or_default();
        let any = !files.is_empty();
        if any {
            *state().files.lock().unwrap_or_else(|e| e.into_inner()) = Some(files);
        }
        set(effect, if any { DROPEFFECT_COPY } else { DROPEFFECT_NONE });
        Ok(())
    }

    fn DragOver(
        &self,
        _keys: MODIFIERKEYS_FLAGS,
        _point: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        let any = state().files.lock().unwrap_or_else(|e| e.into_inner()).is_some();
        set(effect, if any { DROPEFFECT_COPY } else { DROPEFFECT_NONE });
        Ok(())
    }

    fn DragLeave(&self) -> windows::core::Result<()> {
        Ok(())
    }

    /// Let go on the window itself: nothing happens to the files.
    fn Drop(
        &self,
        _data: Ref<IDataObject>,
        _keys: MODIFIERKEYS_FLAGS,
        _point: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        set(effect, DROPEFFECT_NONE);
        if let Some(window) = window() {
            // SAFETY: this runs on the window's own thread.
            unsafe { ShowWindow(window, SW_HIDE) };
        }
        state().probing.store(false, Ordering::Relaxed);
        Ok(())
    }
}

fn set(effect: *mut DROPEFFECT, value: DROPEFFECT) {
    // SAFETY: OLE hands a valid pointer, or none.
    if let Some(effect) = unsafe { effect.as_mut() } {
        *effect = value;
    }
}

/// The paths in a drag's CF_HDROP, if it has one.
fn dropped_files(data: &IDataObject) -> Vec<PathBuf> {
    use std::os::windows::ffi::OsStringExt as _;
    let request = FORMATETC {
        cfFormat: CF_HDROP.0,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    };
    // SAFETY: asks for the data, reads it while held, then gives it back.
    unsafe {
        let Ok(mut medium) = data.GetData(&request) else { return Vec::new() };
        let mut paths = Vec::new();
        if medium.tymed == TYMED_HGLOBAL.0 as u32 {
            let drop = HDROP(medium.u.hGlobal.0);
            let count = DragQueryFileW(drop, u32::MAX, None);
            for index in 0..count {
                let length = DragQueryFileW(drop, index, None) as usize;
                let mut buffer = vec![0u16; length + 1];
                let written = DragQueryFileW(drop, index, Some(&mut buffer)) as usize;
                if written > 0 {
                    paths.push(PathBuf::from(std::ffi::OsString::from_wide(&buffer[..written])));
                }
            }
        }
        ReleaseStgMedium(&mut medium);
        paths
    }
}
