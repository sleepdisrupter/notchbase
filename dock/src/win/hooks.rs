//! What the dock hears from the rest of the desktop, on a thread of its own with its own message loop: windows going
//! down and coming back (WinEvents), presses anywhere (to close a menu, and to catch a minimize button before the
//! window goes), and the keys an open stack takes (a low-level keyboard hook, like the macOS event tap).
//!
//! It also keeps the last picture of the windows people use, since a window that's minimized draws nothing: the
//! front window is pictured when it comes forward and now and then while it stays there, and a window is pictured
//! the moment its minimize button is pressed.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetKeyState, ToUnicode, VK_BACK, VK_CAPITAL, VK_CONTROL, VK_DELETE, VK_DOWN, VK_ESCAPE, VK_LEFT,
    VK_LWIN, VK_MENU, VK_RETURN, VK_RIGHT, VK_RWIN, VK_SHIFT, VK_SPACE, VK_TAB, VK_UP,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_MINIMIZEEND, EVENT_SYSTEM_MINIMIZESTART,
    GA_ROOT, GetAncestor, GetMessageW, HTMINBUTTON, KBDLLHOOKSTRUCT, MSG, MSLLHOOKSTRUCT, OBJID_WINDOW,
    SMTO_ABORTIFHUNG, SendMessageTimeoutW, SetWindowsHookExW, TranslateMessage, WH_KEYBOARD_LL, WH_MOUSE_LL,
    WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS, WM_KEYDOWN, WM_LBUTTONDOWN, WM_NCHITTEST, WM_RBUTTONDOWN,
    WM_SYSKEYDOWN, WindowFromPoint,
};

use super::sys;
use crate::stack::{self, Key};

type Wake = Box<dyn Fn() + Send + Sync>;
static WAKE: OnceLock<Wake> = OnceLock::new();
static STARTED: AtomicBool = AtomicBool::new(false);
/// The dock takes minimizing over (its own animation) while this is on.
static CATCH_MINIMIZE: AtomicBool = AtomicBool::new(false);
static MINIMIZES: Mutex<Vec<(i32, u32)>> = Mutex::new(Vec::new());
static PRESSED_ELSEWHERE: AtomicBool = AtomicBool::new(false);
static WINDOWS_CHANGED: AtomicBool = AtomicBool::new(false);
static CAPTURES: OnceLock<Mutex<Sender<HWND32>>> = OnceLock::new();

/// The last picture of each window, with the frame it had then.
type Pictures = HashMap<u32, (egui::Rect, egui::ColorImage, Instant)>;
static PICTURES: Mutex<Option<Pictures>> = Mutex::new(None);
/// How many windows' pictures are kept, newest first.
const KEPT: usize = 12;
/// The tallest picture kept, in pixels: plenty for a flight, and a 4K window doesn't hold 30 MB.
const KEPT_HEIGHT: usize = 1440;
/// While a window stays in front, it's pictured again this often.
const REFRESH: Duration = Duration::from_millis(2500);

/// A window handle that can cross threads.
#[derive(Clone, Copy)]
struct HWND32(u32);

pub fn on_wake(wake: impl Fn() + Send + Sync + 'static) {
    let _ = WAKE.set(Box::new(wake));
}

fn wake() {
    if let Some(w) = WAKE.get() {
        w();
    }
}

/// Starts listening, once.
pub fn start() {
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let (send, asks) = channel::<HWND32>();
    let _ = CAPTURES.set(Mutex::new(send));
    let _ = std::thread::Builder::new().name("window pictures kept".into()).spawn(move || {
        let mut front: Option<(u32, Instant)> = None;
        loop {
            match asks.recv_timeout(Duration::from_millis(500)) {
                Ok(HWND32(id)) => {
                    // A burst of asks (switching quickly) only pictures the last.
                    let mut id = id;
                    while let Ok(HWND32(next)) = asks.try_recv() {
                        id = next;
                    }
                    keep_picture(id);
                    front = Some((id, Instant::now()));
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    let now = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow() };
                    let id = sys::id_of(now);
                    if !now.is_null() && sys::is_app_window(now) {
                        let due = front.is_none_or(|(was, at)| was != id || at.elapsed() >= REFRESH);
                        if due {
                            keep_picture(id);
                            front = Some((id, Instant::now()));
                        }
                    }
                }
                Err(_) => return,
            }
        }
    });
    let _ = std::thread::Builder::new().name("desktop hooks".into()).spawn(|| unsafe {
        let flags = WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS;
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            std::ptr::null_mut(),
            Some(on_event),
            0,
            0,
            flags,
        );
        SetWinEventHook(
            EVENT_SYSTEM_MINIMIZESTART,
            EVENT_SYSTEM_MINIMIZEEND,
            std::ptr::null_mut(),
            Some(on_event),
            0,
            0,
            flags,
        );
        SetWindowsHookExW(WH_MOUSE_LL, Some(on_mouse), std::ptr::null_mut(), 0);
        SetWindowsHookExW(WH_KEYBOARD_LL, Some(on_keyboard), std::ptr::null_mut(), 0);
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
}

fn ask_picture(hwnd: HWND) {
    if let Some(send) = CAPTURES.get() {
        let _ = send.lock().unwrap_or_else(|e| e.into_inner()).send(HWND32(sys::id_of(hwnd)));
    }
}

fn keep_picture(id: u32) {
    let hwnd = sys::hwnd_of(id);
    if !sys::is_app_window(hwnd) || sys::iconic(hwnd) {
        return;
    }
    let Some(frame) = sys::frame(hwnd).map(sys::rect_of) else { return };
    let Some(image) = super::windows::capture(hwnd) else { return };
    let image = super::windows::smaller(&image, KEPT_HEIGHT);
    let mut kept = PICTURES.lock().unwrap_or_else(|e| e.into_inner());
    let kept = kept.get_or_insert_with(HashMap::new);
    kept.insert(id, (frame, image, Instant::now()));
    if kept.len() > KEPT {
        let oldest = kept.iter().min_by_key(|(_, (_, _, at))| *at).map(|(id, _)| *id);
        if let Some(oldest) = oldest {
            kept.remove(&oldest);
        }
    }
}

/// The last picture taken of window `id` while it was up, and its frame then.
pub fn last_picture(id: u32) -> Option<(egui::Rect, egui::ColorImage)> {
    let kept = PICTURES.lock().unwrap_or_else(|e| e.into_inner());
    kept.as_ref()?.get(&id).map(|(frame, image, _)| (*frame, image.clone()))
}

pub fn forget_picture(id: u32) {
    if let Some(kept) = PICTURES.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        kept.remove(&id);
    }
}

/// Minimizing is the dock's own (no system animation, the dock flies the window down) while on.
pub fn catch_minimize(on: bool) {
    CATCH_MINIMIZE.store(on, Ordering::Relaxed);
}

/// The windows that started going down since the last look, with their apps.
pub fn take_minimizes() -> Vec<(i32, u32)> {
    std::mem::take(&mut *MINIMIZES.lock().unwrap_or_else(|e| e.into_inner()))
}

/// Whether a press landed outside the dock since the last look.
pub fn take_pressed_elsewhere() -> bool {
    PRESSED_ELSEWHERE.swap(false, Ordering::Relaxed)
}

/// Whether a window went down or came back up since the last look.
pub fn take_windows_changed() -> bool {
    WINDOWS_CHANGED.swap(false, Ordering::Relaxed)
}

unsafe extern "system" fn on_event(
    _: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    object: i32,
    _child: i32,
    _thread: u32,
    _time: u32,
) {
    if object != OBJID_WINDOW || hwnd.is_null() {
        return;
    }
    match event {
        EVENT_SYSTEM_FOREGROUND => {
            if sys::is_app_window(hwnd) {
                ask_picture(hwnd);
            }
        }
        EVENT_SYSTEM_MINIMIZESTART => {
            WINDOWS_CHANGED.store(true, Ordering::Relaxed);
            if CATCH_MINIMIZE.load(Ordering::Relaxed) && sys::is_app_window(hwnd) {
                let pid = sys::pid_of(hwnd);
                MINIMIZES.lock().unwrap_or_else(|e| e.into_inner()).push((pid, sys::id_of(hwnd)));
            }
            wake();
        }
        EVENT_SYSTEM_MINIMIZEEND => {
            WINDOWS_CHANGED.store(true, Ordering::Relaxed);
            wake();
        }
        _ => {}
    }
}

unsafe extern "system" fn on_mouse(code: i32, message: WPARAM, data: LPARAM) -> LRESULT {
    if code >= 0 && (message as u32 == WM_LBUTTONDOWN || message as u32 == WM_RBUTTONDOWN) {
        let info = unsafe { &*(data as *const MSLLHOOKSTRUCT) };
        let at = info.pt;
        let under = unsafe { GetAncestor(WindowFromPoint(at), GA_ROOT) };
        let ours = !under.is_null() && sys::pid_of(under) == std::process::id() as i32;
        if !ours {
            PRESSED_ELSEWHERE.store(true, Ordering::Relaxed);
            wake();
            if message as u32 == WM_LBUTTONDOWN && !under.is_null() {
                let id = sys::id_of(under);
                // Asking the window where the press is can wait on the app: never on this hook's thread.
                std::thread::spawn(move || {
                    if on_minimize_button(sys::hwnd_of(id), at) {
                        ask_picture(sys::hwnd_of(id));
                    }
                });
            }
        }
    }
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, message, data) }
}

/// Whether `at` is on the window's minimize button, as the window itself answers.
fn on_minimize_button(hwnd: HWND, at: POINT) -> bool {
    let lparam = ((at.y as u16 as u32) << 16 | (at.x as u16 as u32)) as LPARAM;
    let mut hit: usize = 0;
    let answered = unsafe { SendMessageTimeoutW(hwnd, WM_NCHITTEST, 0, lparam, SMTO_ABORTIFHUNG, 60, &mut hit) };
    answered != 0 && hit as u32 == HTMINBUTTON
}

unsafe extern "system" fn on_keyboard(code: i32, message: WPARAM, data: LPARAM) -> LRESULT {
    let down = message as u32 == WM_KEYDOWN || message as u32 == WM_SYSKEYDOWN;
    if code >= 0 && down && stack::catching_keys() {
        let info = unsafe { &*(data as *const KBDLLHOOKSTRUCT) };
        if let Some((key, swallow)) = key_of(info.vkCode as u16, info.scanCode) {
            stack::push_key(key);
            if swallow {
                return 1;
            }
        }
    }
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, message, data) }
}

fn held(key: u16) -> bool {
    (unsafe { GetAsyncKeyState(i32::from(key)) }) < 0
}

/// What a key does to an open stack, and whether it's kept from the app in front. The Ctrl shortcuts stand in for
/// the Mac's Command ones.
fn key_of(vk: u16, scan: u32) -> Option<(Key, bool)> {
    if held(VK_MENU) || held(VK_LWIN) || held(VK_RWIN) {
        return None;
    }
    if held(VK_CONTROL) {
        return Some(match vk {
            VK_BACK | VK_DELETE => (Key::Trash, true),
            0x41 => (Key::All, true),
            _ => (Key::Away, false),
        });
    }
    let key = match vk {
        VK_ESCAPE => Key::Esc,
        VK_RETURN => Key::Open,
        VK_SPACE => Key::Look,
        VK_BACK => Key::Back,
        VK_DELETE => Key::Trash,
        VK_LEFT => Key::Left,
        VK_RIGHT => Key::Right,
        VK_DOWN => Key::Down,
        VK_UP => Key::Up,
        VK_TAB | VK_SHIFT | VK_CONTROL | VK_MENU | VK_CAPITAL => return None,
        _ => {
            let text = typed(vk, scan)?;
            Key::Text(text)
        }
    };
    Some((key, true))
}

/// The character a key types with Shift and Caps Lock as they are.
fn typed(vk: u16, scan: u32) -> Option<String> {
    let mut state = [0u8; 256];
    if held(VK_SHIFT) {
        state[VK_SHIFT as usize] = 0x80;
    }
    if unsafe { GetKeyState(i32::from(VK_CAPITAL)) } & 1 != 0 {
        state[VK_CAPITAL as usize] = 1;
    }
    let mut out = [0u16; 8];
    // Flag 4: don't change the keyboard's dead-key state.
    let n = unsafe { ToUnicode(u32::from(vk), scan, state.as_ptr(), out.as_mut_ptr(), out.len() as i32, 4) };
    if n <= 0 {
        return None;
    }
    let text = String::from_utf16_lossy(&out[..n as usize]);
    text.chars().all(|c| !c.is_control()).then_some(text)
}
