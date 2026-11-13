//! The Windows key through a low-level keyboard hook on its own thread. Keys it replays are marked so it lets them by.

use std::cell::RefCell;
use std::ptr::{null, null_mut};
use std::sync::mpsc::{self, SyncSender};
use std::thread::JoinHandle;

use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY, SendInput, VK_CONTROL,
    VK_MENU, VK_SHIFT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, HC_ACTION, KBDLLHOOKSTRUCT, LLKHF_EXTENDED, MSG, PM_NOREMOVE, PeekMessageW,
    PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL, WM_KEYDOWN, WM_QUIT, WM_SYSKEYDOWN,
};

use super::Pressed;
use super::lone::{LEFT_WIN, Lone, RIGHT_WIN, Stroke, Verdict};

const THREAD: &str = "notchbase summon key";
const NO_HOOK: &str = "couldn't watch the Windows key";
const OURS: usize = 0x6E62_6B79;
const TAKEN: LRESULT = 1;

pub struct KeyTap {
    thread: u32,
    join: Option<JoinHandle<()>>,
}

impl KeyTap {
    pub fn install(pressed: Pressed) -> Result<Self, String> {
        let (ready_tx, ready) = mpsc::sync_channel(1);
        let join = std::thread::Builder::new()
            .name(THREAD.into())
            .spawn(move || hook_thread(Hooked { pressed, lone: Lone::default() }, &ready_tx))
            .map_err(|err| err.to_string())?;
        match ready.recv() {
            Ok(Some(thread)) => Ok(Self { thread, join: Some(join) }),
            _ => {
                let _ = join.join();
                Err(NO_HOOK.into())
            }
        }
    }
}

impl Drop for KeyTap {
    fn drop(&mut self) {
        unsafe { PostThreadMessageW(self.thread, WM_QUIT, 0, 0) };
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

struct Hooked {
    pressed: Pressed,
    lone: Lone,
}

thread_local! {
    static HOOKED: RefCell<Option<Hooked>> = const { RefCell::new(None) };
}

fn hook_thread(hooked: Hooked, ready: &SyncSender<Option<u32>>) {
    HOOKED.set(Some(hooked));
    let mut msg = MSG::default();
    unsafe { PeekMessageW(&mut msg, null_mut(), 0, 0, PM_NOREMOVE) };
    let module = unsafe { GetModuleHandleW(null()) };
    let hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), module, 0) };
    if hook.is_null() {
        let _ = ready.send(None);
        return;
    }
    let _ = ready.send(Some(unsafe { GetCurrentThreadId() }));
    while unsafe { GetMessageW(&mut msg, null_mut(), 0, 0) } > 0 {}
    unsafe { UnhookWindowsHookEx(hook) };
    HOOKED.set(None);
}

fn pressed_key(code: u32, scan: u32, extended: bool) -> INPUT {
    let flags = if extended { KEYEVENTF_EXTENDEDKEY } else { 0 };
    let key = KEYBDINPUT { wVk: code as u16, wScan: scan as u16, dwFlags: flags, time: 0, dwExtraInfo: OURS };
    INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki: key } }
}

fn replay(windows_key: u32, beside: &KBDLLHOOKSTRUCT) {
    let extended = beside.flags & LLKHF_EXTENDED != 0;
    let strokes = [pressed_key(windows_key, windows_key, true), pressed_key(beside.vkCode, beside.scanCode, extended)];
    unsafe { SendInput(strokes.len() as u32, strokes.as_ptr(), size_of::<INPUT>() as i32) };
}

fn held(key: u32) -> bool {
    let state = unsafe { GetAsyncKeyState(key as i32) };
    state < 0
}

fn stroke(info: &KBDLLHOOKSTRUCT, down: bool) -> Stroke {
    let windows_key = matches!(info.vkCode, LEFT_WIN | RIGHT_WIN);
    Stroke {
        code: info.vkCode,
        down,
        ours: info.dwExtraInfo == OURS,
        modified: windows_key && [VK_CONTROL, VK_MENU, VK_SHIFT].into_iter().any(|key| held(u32::from(key))),
        system_holds: windows_key && held(info.vkCode),
    }
}

fn seen(stroke: Stroke) -> (Verdict, Option<Pressed>) {
    HOOKED.with(|cell| {
        let Ok(mut hooked) = cell.try_borrow_mut() else { return (Verdict::Pass, None) };
        let Some(hooked) = hooked.as_mut() else { return (Verdict::Pass, None) };
        let verdict = hooked.lone.see(stroke);
        (verdict, (verdict == Verdict::Summon).then(|| hooked.pressed.clone()))
    })
}

unsafe extern "system" fn keyboard_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let info = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
        let down = matches!(wparam as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
        match seen(stroke(info, down)) {
            (Verdict::Pass, _) => {}
            (Verdict::Swallow, _) => return TAKEN,
            (Verdict::Chord(windows_key), _) => {
                replay(windows_key, info);
                return TAKEN;
            }
            (Verdict::Summon, pressed) => {
                if let Some(pressed) = pressed {
                    pressed();
                }
                return TAKEN;
            }
        }
    }
    unsafe { CallNextHookEx(null_mut(), code, wparam, lparam) }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{VK_LWIN, VK_RWIN};

    use super::*;

    #[test]
    fn replays_are_marked_as_ours() {
        assert_eq!((LEFT_WIN, RIGHT_WIN), (u32::from(VK_LWIN), u32::from(VK_RWIN)));
        let windows_key = unsafe { pressed_key(LEFT_WIN, LEFT_WIN, true).Anonymous.ki };
        assert_eq!(
            (windows_key.wVk, windows_key.dwFlags, windows_key.dwExtraInfo),
            (VK_LWIN, KEYEVENTF_EXTENDEDKEY, OURS)
        );
        let letter = unsafe { pressed_key(0x45, 0x12, false).Anonymous.ki };
        assert_eq!((letter.wVk, letter.wScan, letter.dwFlags, letter.dwExtraInfo), (0x45, 0x12, 0, OURS));
    }

    #[test]
    #[ignore = "hooks the keyboard of this machine briefly"]
    fn hook_installs_and_lets_go() {
        let tap = KeyTap::install(Arc::new(|| {})).expect("the hook installs");
        drop(tap);
    }
}
