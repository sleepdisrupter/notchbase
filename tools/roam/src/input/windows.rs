//! Windows: low-level mouse and keyboard hooks hear input (and swallow it while the cursor is on
//! another computer), and SendInput plays input back. While the cursor is away, Windows' own
//! cursor waits in the middle of the main screen under a tiny invisible window that hides it.
//!
//! For low latency, as game streaming apps do: the timer resolution goes to 1 ms and Wi-Fi is
//! put in media streaming mode with background scans off while a session runs. Background scans
//! are the usual cause of a Wi-Fi link stalling for a moment every minute or so.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

use windows_sys::Win32::Foundation::{FALSE, HANDLE, HWND, LPARAM, LRESULT, POINT, RECT, TRUE, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO};
use windows_sys::Win32::Media::{timeBeginPeriod, timeEndPeriod};
use windows_sys::Win32::NetworkManagement::WiFi::{
    WLAN_INTERFACE_INFO_LIST, WlanCloseHandle, WlanEnumInterfaces, WlanFreeMemory, WlanOpenHandle, WlanSetInterface,
    wlan_intf_opcode_background_scan_enabled, wlan_intf_opcode_media_streaming_mode,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::{
    GetCurrentThread, GetCurrentThreadId, SetThreadPriority, THREAD_PRIORITY_HIGHEST, THREAD_PRIORITY_TIME_CRITICAL,
};
use windows_sys::Win32::UI::HiDpi::GetDpiForSystem;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP,
    KEYEVENTF_SCANCODE, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_MOVE_NOCOALESCE, MOUSEEVENTF_RIGHTDOWN,
    MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_VIRTUALDESK, MOUSEEVENTF_WHEEL, MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, MOUSEINPUT,
    SendInput,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, GetSystemMetrics, HC_ACTION,
    KBDLLHOOKSTRUCT, LLKHF_EXTENDED, LLKHF_INJECTED, LLKHF_UP, LLMHF_INJECTED, LWA_ALPHA, MSG, MSLLHOOKSTRUCT,
    PostMessageW, PostThreadMessageW, RegisterClassW, SM_CXSCREEN, SM_CXVIRTUALSCREEN, SM_CYSCREEN, SM_CYVIRTUALSCREEN,
    SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SW_HIDE, SWP_NOACTIVATE, SWP_SHOWWINDOW, SetCursor, SetCursorPos,
    SetLayeredWindowAttributes, SetWindowPos, SetWindowsHookExW, ShowWindowAsync, TranslateMessage,
    UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_APP, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_QUIT, WM_RBUTTONDOWN, WM_RBUTTONUP,
    WM_SETCURSOR, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_XBUTTONDOWN, WM_XBUTTONUP, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};
use windows_sys::core::BOOL;

use super::{Input, Platform, Play, Sink, Watchdog};
use crate::keys;
use crate::layout::Rect;

/// Marks the input Roam sends, so nothing mistakes it for the person's own.
pub(super) const MARK: usize = 0x526F_616D;
const COVER: i32 = 64;
const HWND_TOPMOST: HWND = -1isize as HWND;
/// Asks the hook thread to put the cover under the parked cursor.
const PARK: u32 = WM_APP + 1;

/// What the hooks need. Hooks are plain functions, so this is global.
struct Hooks {
    sink: Mutex<Option<Sink>>,
    captured: AtomicBool,
    park: [AtomicI32; 2],
    last: [AtomicI32; 2],
    local: Mutex<HashSet<u16>>,
    thread: AtomicU32,
    cover: Mutex<usize>,
}

fn hooks() -> &'static Hooks {
    static HOOKS: OnceLock<Hooks> = OnceLock::new();
    HOOKS.get_or_init(|| Hooks {
        sink: Mutex::new(None),
        captured: AtomicBool::new(false),
        park: [AtomicI32::new(0), AtomicI32::new(0)],
        last: [AtomicI32::new(0), AtomicI32::new(0)],
        local: Mutex::new(HashSet::new()),
        thread: AtomicU32::new(0),
        cover: Mutex::new(0),
    })
}

pub fn urgent_thread() {
    // SAFETY: changes only the calling thread's priority.
    unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST) };
}

static WATCHDOG: Watchdog = Watchdog::new();

/// Whether input is captured now. If Roam stopped checking in, the capture ends here and then,
/// so the mouse and keyboard can never stay locked.
fn captured_now() -> bool {
    let state = hooks();
    if !state.captured.load(Ordering::Acquire) {
        return false;
    }
    if WATCHDOG.stale() {
        state.captured.store(false, Ordering::Release);
        let cover = *state.cover.lock().unwrap_or_else(|e| e.into_inner()) as HWND;
        if !cover.is_null() {
            // SAFETY: hides the cursor cover without waiting on its thread.
            unsafe { ShowWindowAsync(cover, SW_HIDE) };
        }
        return false;
    }
    true
}

fn send(input: Input) {
    if let Some(sink) = hooks().sink.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        sink(input);
    }
}

pub struct Windows {
    buttons: u8,
    wheel_carry: [f32; 2],
    fast: Option<Fast>,
}

struct Fast {
    wlan: HANDLE,
}

// The WLAN handle is only used and closed by the engine thread that owns this.
unsafe impl Send for Fast {}

impl Windows {
    pub fn new() -> Self {
        Self { buttons: 0, wheel_carry: [0.0; 2], fast: None }
    }

    fn inject(inputs: &[INPUT]) {
        // SAFETY: a slice of fully initialised INPUT structures.
        unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32) };
    }

    fn mouse(dx: i32, dy: i32, data: i32, flags: u32) -> INPUT {
        INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT { dx, dy, mouseData: data as u32, dwFlags: flags, time: 0, dwExtraInfo: MARK },
            },
        }
    }
}

unsafe extern "system" fn mouse_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code != HC_ACTION as i32 {
        // SAFETY: passing the hook on unchanged.
        return unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) };
    }
    // SAFETY: for WH_MOUSE_LL, lparam points to an MSLLHOOKSTRUCT.
    let info = unsafe { &*(lparam as *const MSLLHOOKSTRUCT) };
    let state = hooks();
    let captured = captured_now();
    if info.flags & LLMHF_INJECTED != 0 {
        // SAFETY: as above.
        return unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) };
    }
    let message = wparam as u32;
    match message {
        WM_MOUSEMOVE => {
            let (x, y) = (info.pt.x, info.pt.y);
            if captured {
                // SAFETY: the cursor is over this thread's cover window.
                unsafe { SetCursor(std::ptr::null_mut()) };
                let park = [state.park[0].load(Ordering::Relaxed), state.park[1].load(Ordering::Relaxed)];
                // A move made just before the cursor was parked is still measured from where it
                // was at home, not from the park, or the cursor would jump across the screen.
                let home = [state.last[0].load(Ordering::Relaxed), state.last[1].load(Ordering::Relaxed)];
                let far = |from: [i32; 2]| (x - from[0]).abs() + (y - from[1]).abs();
                let from = if far(home) < far(park) { home } else { park };
                let delta = [f64::from(x - from[0]), f64::from(y - from[1])];
                if delta != [0.0, 0.0] {
                    send(Input::Move { at: [f64::from(park[0]), f64::from(park[1])], delta });
                }
            } else {
                let before = [state.last[0].swap(x, Ordering::Relaxed), state.last[1].swap(y, Ordering::Relaxed)];
                let delta = [f64::from(x - before[0]), f64::from(y - before[1])];
                send(Input::Move { at: [f64::from(x), f64::from(y)], delta });
            }
        }
        WM_LBUTTONDOWN | WM_LBUTTONUP | WM_RBUTTONDOWN | WM_RBUTTONUP | WM_MBUTTONDOWN | WM_MBUTTONUP
        | WM_XBUTTONDOWN | WM_XBUTTONUP => {
            let button = match message {
                WM_LBUTTONDOWN | WM_LBUTTONUP => 0,
                WM_RBUTTONDOWN | WM_RBUTTONUP => 1,
                WM_MBUTTONDOWN | WM_MBUTTONUP => 2,
                _ if (info.mouseData >> 16) & 0xFFFF == 2 => 4,
                _ => 3,
            };
            let down = matches!(message, WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN);
            send(Input::Button { button, down });
        }
        WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
            let amount = f32::from((info.mouseData >> 16) as u16 as i16);
            let (dx, dy) = if message == WM_MOUSEWHEEL { (0.0, amount) } else { (amount, 0.0) };
            send(Input::Wheel { dx, dy });
        }
        _ => {}
    }
    if captured {
        return 1;
    }
    // SAFETY: as above.
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
}

unsafe extern "system" fn keyboard_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code != HC_ACTION as i32 {
        // SAFETY: passing the hook on unchanged.
        return unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) };
    }
    // SAFETY: for WH_KEYBOARD_LL, lparam points to a KBDLLHOOKSTRUCT.
    let info = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
    let state = hooks();
    let pass = || unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) };
    if info.flags & LLKHF_INJECTED != 0 {
        return pass();
    }
    let message = wparam as u32;
    if !matches!(message, WM_KEYDOWN | WM_KEYUP | WM_SYSKEYDOWN | WM_SYSKEYUP) {
        return pass();
    }
    let down = info.flags & LLKHF_UP == 0;
    let Some(hid) = keys::from_windows(info.scanCode as u16, info.flags & LLKHF_EXTENDED != 0) else {
        return if captured_now() { 1 } else { pass() };
    };
    let captured = captured_now();
    let mut local = state.local.lock().unwrap_or_else(|e| e.into_inner());
    if !captured {
        if down {
            local.insert(hid);
        } else {
            local.remove(&hid);
        }
        drop(local);
        return pass();
    }
    // Pressed before the cursor left: its release stays here.
    if !down && local.remove(&hid) {
        drop(local);
        return pass();
    }
    drop(local);
    send(Input::Key { hid, down });
    1
}

unsafe extern "system" fn cover_proc(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if message == PARK {
        let state = hooks();
        if state.captured.load(Ordering::Acquire) {
            let (x, y) = (state.park[0].load(Ordering::Relaxed), state.park[1].load(Ordering::Relaxed));
            // SAFETY: this thread owns the cover. Placing it first, then the cursor on it, makes
            // Windows ask the cover for the cursor, which is none.
            unsafe {
                let flags = SWP_NOACTIVATE | SWP_SHOWWINDOW;
                SetWindowPos(window, HWND_TOPMOST, x - COVER / 2, y - COVER / 2, COVER, COVER, flags);
                SetCursorPos(x, y);
                SetCursor(std::ptr::null_mut());
            }
        }
        return 0;
    }
    if message == WM_SETCURSOR {
        // SAFETY: no cursor while over this window.
        unsafe { SetCursor(std::ptr::null_mut()) };
        return 1;
    }
    // SAFETY: everything else gets the default handling.
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn make_cover() -> HWND {
    let class = wide("RoamCursorCover");
    // SAFETY: registering a window class with a valid procedure and name, then making a tiny
    // invisible tool window of it on this (the hook) thread, which pumps its messages.
    unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let mut info: WNDCLASSW = std::mem::zeroed();
        info.lpfnWndProc = Some(cover_proc);
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
            COVER,
            COVER,
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

impl Platform for Windows {
    fn screens(&mut self) -> (Vec<Rect>, f64) {
        unsafe extern "system" fn each(monitor: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
            // SAFETY: `data` is the Vec passed below; `monitor` is valid during the callback.
            unsafe {
                let found = &mut *(data as *mut Vec<Rect>);
                let mut info: MONITORINFO = std::mem::zeroed();
                info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
                if GetMonitorInfoW(monitor, &mut info) != 0 {
                    let r = info.rcMonitor;
                    found.push(Rect::new(
                        f64::from(r.left),
                        f64::from(r.top),
                        f64::from(r.right - r.left),
                        f64::from(r.bottom - r.top),
                    ));
                }
            }
            TRUE
        }
        let mut found: Vec<Rect> = Vec::new();
        // SAFETY: the callback only writes into `found`, which outlives the call.
        unsafe {
            EnumDisplayMonitors(std::ptr::null_mut(), std::ptr::null(), Some(each), &mut found as *mut _ as LPARAM)
        };
        if found.is_empty() {
            // SAFETY: plain metrics.
            let (w, h) = unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) };
            found.push(Rect::new(0.0, 0.0, f64::from(w.max(1)), f64::from(h.max(1))));
        }
        // SAFETY: plain query.
        let dpi = unsafe { GetDpiForSystem() };
        let ppu = if dpi == 0 { 1.0 } else { f64::from(dpi) / 96.0 };
        (found, ppu)
    }

    fn listen(&mut self, sink: Sink) -> Result<(), String> {
        let state = hooks();
        let [x, y] = cursor();
        state.last[0].store(x as i32, Ordering::Relaxed);
        state.last[1].store(y as i32, Ordering::Relaxed);
        *state.sink.lock().unwrap_or_else(|e| e.into_inner()) = Some(sink);
        if state.thread.load(Ordering::Acquire) != 0 {
            return Ok(());
        }
        let (ready, started) = std::sync::mpsc::channel::<bool>();
        let spawned = std::thread::Builder::new().name("roam-hooks".into()).spawn(move || {
            // SAFETY: hooks are installed and removed on this thread, which pumps messages for
            // them (and for the cover window) until told to quit.
            unsafe {
                SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL);
                let instance = GetModuleHandleW(std::ptr::null());
                let mouse = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), instance, 0);
                let keyboard = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), instance, 0);
                if mouse.is_null() || keyboard.is_null() {
                    if !mouse.is_null() {
                        UnhookWindowsHookEx(mouse);
                    }
                    if !keyboard.is_null() {
                        UnhookWindowsHookEx(keyboard);
                    }
                    let _ = ready.send(false);
                    return;
                }
                *hooks().cover.lock().unwrap_or_else(|e| e.into_inner()) = make_cover() as usize;
                hooks().thread.store(GetCurrentThreadId(), Ordering::Release);
                let _ = ready.send(true);
                let mut message: MSG = std::mem::zeroed();
                while GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) > 0 {
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
                UnhookWindowsHookEx(mouse);
                UnhookWindowsHookEx(keyboard);
                hooks().thread.store(0, Ordering::Release);
            }
        });
        if spawned.is_err() || !started.recv().unwrap_or(false) {
            return Err("Roam couldn't start listening to the mouse and keyboard".into());
        }
        Ok(())
    }

    fn stop_listening(&mut self) {
        let state = hooks();
        self.capture(false);
        *state.sink.lock().unwrap_or_else(|e| e.into_inner()) = None;
        let thread = state.thread.load(Ordering::Acquire);
        if thread != 0 {
            // SAFETY: asks the hook thread's message loop to end.
            unsafe { PostThreadMessageW(thread, WM_QUIT, 0, 0) };
        }
    }

    fn capture(&mut self, on: bool) {
        let state = hooks();
        let cover = *state.cover.lock().unwrap_or_else(|e| e.into_inner()) as HWND;
        if on {
            WATCHDOG.pet();
            // SAFETY: plain metrics and cursor calls.
            let (x, y) = unsafe { (GetSystemMetrics(SM_CXSCREEN) / 2, GetSystemMetrics(SM_CYSCREEN) / 2) };
            state.park[0].store(x, Ordering::Relaxed);
            state.park[1].store(y, Ordering::Relaxed);
            state.captured.store(true, Ordering::Release);
            // SAFETY: plain cursor call, and a message to the hook thread, which owns the cover
            // and places it without this thread waiting.
            unsafe {
                SetCursorPos(x, y);
                if !cover.is_null() {
                    PostMessageW(cover, PARK, 0, 0);
                }
            }
        } else {
            state.captured.store(false, Ordering::Release);
            if !cover.is_null() {
                // SAFETY: as above.
                unsafe { ShowWindowAsync(cover, SW_HIDE) };
            }
        }
    }

    fn warp(&mut self, at: [f64; 2]) {
        let (x, y) = (at[0].round() as i32, at[1].round() as i32);
        let state = hooks();
        state.last[0].store(x, Ordering::Relaxed);
        state.last[1].store(y, Ordering::Relaxed);
        // SAFETY: plain cursor call.
        unsafe { SetCursorPos(x, y) };
    }

    fn play(&mut self, play: Play) {
        match play {
            Play::Move(at) => {
                // SAFETY: plain metrics.
                let (left, top, width, height) = unsafe {
                    (
                        GetSystemMetrics(SM_XVIRTUALSCREEN),
                        GetSystemMetrics(SM_YVIRTUALSCREEN),
                        GetSystemMetrics(SM_CXVIRTUALSCREEN).max(2),
                        GetSystemMetrics(SM_CYVIRTUALSCREEN).max(2),
                    )
                };
                let nx = ((at[0] - f64::from(left)) * 65535.0 / f64::from(width - 1)).round() as i32;
                let ny = ((at[1] - f64::from(top)) * 65535.0 / f64::from(height - 1)).round() as i32;
                let flags =
                    MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK | MOUSEEVENTF_MOVE_NOCOALESCE;
                Self::inject(&[Self::mouse(nx, ny, 0, flags)]);
            }
            Play::Button { button, down } => {
                let (flags, data) = match (button, down) {
                    (0, true) => (MOUSEEVENTF_LEFTDOWN, 0),
                    (0, false) => (MOUSEEVENTF_LEFTUP, 0),
                    (1, true) => (MOUSEEVENTF_RIGHTDOWN, 0),
                    (1, false) => (MOUSEEVENTF_RIGHTUP, 0),
                    (2, true) => (MOUSEEVENTF_MIDDLEDOWN, 0),
                    (2, false) => (MOUSEEVENTF_MIDDLEUP, 0),
                    (b, true) => (MOUSEEVENTF_XDOWN, if b == 4 { 2 } else { 1 }),
                    (b, false) => (MOUSEEVENTF_XUP, if b == 4 { 2 } else { 1 }),
                };
                if down {
                    self.buttons |= 1 << button;
                } else {
                    self.buttons &= !(1 << button);
                }
                Self::inject(&[Self::mouse(0, 0, data, flags)]);
            }
            Play::Wheel { dx, dy } => {
                self.wheel_carry[0] += dx;
                self.wheel_carry[1] += dy;
                let whole = [self.wheel_carry[0].trunc(), self.wheel_carry[1].trunc()];
                self.wheel_carry[0] -= whole[0];
                self.wheel_carry[1] -= whole[1];
                let mut inputs = Vec::new();
                if whole[1] != 0.0 {
                    inputs.push(Self::mouse(0, 0, whole[1] as i32, MOUSEEVENTF_WHEEL));
                }
                if whole[0] != 0.0 {
                    inputs.push(Self::mouse(0, 0, whole[0] as i32, MOUSEEVENTF_HWHEEL));
                }
                Self::inject(&inputs);
            }
            Play::Key { hid, down } => {
                let Some((scan, extended)) = keys::to_windows(hid) else { return };
                let mut flags = KEYEVENTF_SCANCODE;
                if extended {
                    flags |= KEYEVENTF_EXTENDEDKEY;
                }
                if !down {
                    flags |= KEYEVENTF_KEYUP;
                }
                let input = INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT { wVk: 0, wScan: scan, dwFlags: flags, time: 0, dwExtraInfo: MARK },
                    },
                };
                Self::inject(&[input]);
            }
        }
    }

    fn alive(&mut self) {
        WATCHDOG.pet();
    }

    fn still_captured(&mut self) -> bool {
        captured_now()
    }

    fn left_button(&mut self, down: bool) {
        super::windows_drag::left_button(down);
    }

    fn dragged_files(&mut self, at: [f64; 2]) -> Option<Vec<std::path::PathBuf>> {
        super::windows_drag::dragged_files(at)
    }

    fn cancel_drag(&mut self) {
        super::windows_drag::cancel_drag();
    }

    fn drag_in(&mut self, paths: Vec<std::path::PathBuf>, played: bool) -> bool {
        let started = super::windows_drag::drag_in(paths);
        if started && played {
            // The left button goes down here, and the controller's letting go plays its release.
            self.buttons |= 1;
        }
        started
    }

    fn fast(&mut self, on: bool) {
        if on && self.fast.is_none() {
            // SAFETY: timer and WLAN calls with valid arguments; the WLAN handle is kept open
            // because Windows undoes these settings when it closes.
            unsafe {
                timeBeginPeriod(1);
                let mut version = 0u32;
                let mut wlan: HANDLE = std::ptr::null_mut();
                if WlanOpenHandle(2, std::ptr::null(), &mut version, &mut wlan) == 0 {
                    set_wifi(wlan, FALSE, TRUE);
                }
                self.fast = Some(Fast { wlan });
            }
        } else if !on && let Some(Fast { wlan }) = self.fast.take() {
            // SAFETY: undoing the above.
            unsafe {
                if !wlan.is_null() {
                    set_wifi(wlan, TRUE, FALSE);
                    WlanCloseHandle(wlan, std::ptr::null());
                }
                timeEndPeriod(1);
            }
        }
    }
}

/// Turns Wi-Fi background scanning and media streaming mode on or off on every adapter.
unsafe fn set_wifi(wlan: HANDLE, background_scan: BOOL, streaming: BOOL) {
    let mut list: *mut WLAN_INTERFACE_INFO_LIST = std::ptr::null_mut();
    // SAFETY: the list is allocated by WLAN and freed below.
    unsafe {
        if WlanEnumInterfaces(wlan, std::ptr::null(), &mut list) != 0 || list.is_null() {
            return;
        }
        let count = (*list).dwNumberOfItems as usize;
        let first = (*list).InterfaceInfo.as_ptr();
        for index in 0..count {
            let guid = &(*first.add(index)).InterfaceGuid;
            let size = std::mem::size_of::<BOOL>() as u32;
            WlanSetInterface(
                wlan,
                guid,
                wlan_intf_opcode_background_scan_enabled,
                size,
                &background_scan as *const BOOL as *const _,
                std::ptr::null(),
            );
            WlanSetInterface(
                wlan,
                guid,
                wlan_intf_opcode_media_streaming_mode,
                size,
                &streaming as *const BOOL as *const _,
                std::ptr::null(),
            );
        }
        WlanFreeMemory(list as *const _);
    }
}

impl Drop for Windows {
    fn drop(&mut self) {
        self.fast(false);
        self.stop_listening();
    }
}

fn cursor() -> [f64; 2] {
    let mut point = POINT { x: 0, y: 0 };
    // SAFETY: writes the cursor position into `point`.
    unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut point) };
    [f64::from(point.x), f64::from(point.y)]
}
