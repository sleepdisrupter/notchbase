//! Windows: a low-level keyboard hook hears Ctrl+Win (or the right Ctrl or Alt), WASAPI records
//! the microphone, and the words are typed as Unicode keystrokes, so the clipboard is left alone.
//! The fn key on laptops never reaches Windows, which is why the key differs from the Mac.

use std::cell::RefCell;
use std::path::Path;
use std::ptr::{null, null_mut};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use notchbase_wasapi::{Apartment, Audio, CHANNELS, RATE as DEVICE_RATE};
use windows_sys::Win32::Foundation::{CloseHandle, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::{
    GetCurrentThreadId, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, SendInput,
    VK_ESCAPE, VK_LCONTROL, VK_LWIN, VK_RCONTROL, VK_RETURN, VK_RMENU, VK_RWIN,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetForegroundWindow, GetMessageW, GetWindowTextW, GetWindowThreadProcessId, HC_ACTION,
    KBDLLHOOKSTRUCT, MSG, PM_NOREMOVE, PeekMessageW, PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx,
    WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

use super::{Cue, Hotkey, KeySink, Platform, Problem, Recording, Surroundings, Tape, Typed};
use crate::sound::{Converter, level};
use crate::trigger::Key;

/// Marks the keys dictation sends, so its own hook lets them by.
const OURS: usize = 0x6E62_6474;
const TAKEN: LRESULT = 1;
/// An unassigned key, pressed between the Windows key's press and release so the Start menu
/// (or, after Alt, the menu bar) doesn't open.
const MASK_KEY: u16 = 0xE8;
const POLL: Duration = Duration::from_millis(10);

#[derive(Default)]
pub struct Native {
    tap: Option<KeyTap>,
}

impl Platform for Native {
    fn watch(&mut self, hotkey: Hotkey, keys: KeySink, listening: Arc<AtomicBool>) -> Result<(), Problem> {
        self.tap = None;
        let tap = KeyTap::install(Hooked { chord: Chord::new(hotkey), keys, listening }).ok_or_else(|| Problem {
            says: "Couldn't watch the dictation key".into(),
            fix: None,
            button: "",
            blocks: false,
        })?;
        self.tap = Some(tap);
        Ok(())
    }

    fn hotkey_down(&self, hotkey: Hotkey) -> Option<bool> {
        Some(codes(hotkey).iter().any(|&code| held(code)))
    }

    fn listen(&mut self, tape: Tape) -> Result<Box<dyn Recording>, String> {
        Microphone::start(tape).map(|microphone| Box::new(microphone) as Box<dyn Recording>)
    }

    fn surroundings(&self, _read_text: bool) -> Surroundings {
        let window = unsafe { GetForegroundWindow() };
        if window.is_null() {
            return Surroundings::default();
        }
        let mut title = [0u16; 512];
        let length = unsafe { GetWindowTextW(window, title.as_mut_ptr(), title.len() as i32) };
        let visible = String::from_utf16_lossy(&title[..length.max(0) as usize]);
        Surroundings { app: app_of(window), before: None, after: None, visible }
    }

    fn type_text(&self, text: &str, _reread: bool) -> Result<Typed, String> {
        let inputs = keystrokes(text);
        if inputs.is_empty() {
            return Ok(Typed { reread: None });
        }
        let sent = unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), size_of::<INPUT>() as i32) };
        if sent == 0 { Err("Windows didn't take the keystrokes".into()) } else { Ok(Typed { reread: None }) }
    }

    fn cue(&self, _cue: Cue) {}

    fn problem(&self, _hotkey: Hotkey) -> Option<Problem> {
        None
    }
}

fn app_of(window: windows_sys::Win32::Foundation::HWND) -> String {
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(window, &mut pid) };
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return String::new();
    }
    let mut path = [0u16; 1024];
    let mut length = path.len() as u32;
    let ok = unsafe { QueryFullProcessImageNameW(process, 0, path.as_mut_ptr(), &mut length) };
    unsafe { CloseHandle(process) };
    if ok == 0 {
        return String::new();
    }
    let path = String::from_utf16_lossy(&path[..length as usize]);
    Path::new(&path).file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default()
}

fn key(code: u16, scan: u16, flags: u32) -> INPUT {
    let key = KEYBDINPUT { wVk: code, wScan: scan, dwFlags: flags, time: 0, dwExtraInfo: OURS };
    INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki: key } }
}

fn keystrokes(text: &str) -> Vec<INPUT> {
    let mut inputs = Vec::with_capacity(text.len() * 2);
    for c in text.chars() {
        if c == '\n' {
            inputs.push(key(VK_RETURN, 0, 0));
            inputs.push(key(VK_RETURN, 0, KEYEVENTF_KEYUP));
            continue;
        }
        if c == '\r' {
            continue;
        }
        let mut units = [0u16; 2];
        for unit in c.encode_utf16(&mut units) {
            inputs.push(key(0, *unit, KEYEVENTF_UNICODE));
            inputs.push(key(0, *unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP));
        }
    }
    inputs
}

fn held(code: u32) -> bool {
    unsafe { GetAsyncKeyState(code as i32) < 0 }
}

const LEFT_CTRL: u32 = VK_LCONTROL as u32;
const RIGHT_CTRL: u32 = VK_RCONTROL as u32;
const LEFT_WIN: u32 = VK_LWIN as u32;
const RIGHT_WIN: u32 = VK_RWIN as u32;
const RIGHT_ALT: u32 = VK_RMENU as u32;
const ESCAPE: u32 = VK_ESCAPE as u32;

fn codes(hotkey: Hotkey) -> &'static [u32] {
    match hotkey {
        Hotkey::RightCtrl => &[RIGHT_CTRL],
        Hotkey::RightAlt => &[RIGHT_ALT],
        _ => &[LEFT_CTRL, RIGHT_CTRL, LEFT_WIN, RIGHT_WIN],
    }
}

/// Follows the keys of the dictation chord as they go down and up.
#[derive(Debug)]
struct Chord {
    hotkey: Hotkey,
    ctrl: bool,
    win: bool,
    single: bool,
    active: bool,
    /// The chord was used while the Windows key (or Alt) was down, so its release needs masking.
    used: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum Seen {
    Pass(Option<Key>),
    /// Send the mask key first, then let the release through.
    Masked(Option<Key>),
    Swallow(Key),
}

impl Chord {
    fn new(hotkey: Hotkey) -> Self {
        Self { hotkey, ctrl: false, win: false, single: false, active: false, used: false }
    }

    fn see(&mut self, code: u32, down: bool, listening: bool) -> Seen {
        if down && code == ESCAPE && listening {
            return Seen::Swallow(Key::Escape);
        }
        let was = self.active;
        match (self.hotkey, code) {
            (Hotkey::RightCtrl, RIGHT_CTRL) | (Hotkey::RightAlt, RIGHT_ALT) => self.single = down,
            (Hotkey::RightCtrl | Hotkey::RightAlt, _) => return self.other(down),
            (_, LEFT_CTRL | RIGHT_CTRL) => self.ctrl = down,
            (_, LEFT_WIN | RIGHT_WIN) => self.win = down,
            _ => return self.other(down),
        }
        self.active = match self.hotkey {
            Hotkey::RightCtrl | Hotkey::RightAlt => self.single,
            _ => self.ctrl && self.win,
        };
        let key = match (was, self.active) {
            (false, true) => Some(Key::Down),
            (true, false) => Some(Key::Up),
            _ => None,
        };
        if self.active {
            self.used = true;
        }
        let releases_mask = !down && self.used && matches!(code, LEFT_WIN | RIGHT_WIN | RIGHT_ALT);
        if releases_mask {
            self.used = false;
            return Seen::Masked(key);
        }
        Seen::Pass(key)
    }

    fn other(&mut self, down: bool) -> Seen {
        Seen::Pass((down && self.active).then_some(Key::Other))
    }
}

struct Hooked {
    chord: Chord,
    keys: KeySink,
    listening: Arc<AtomicBool>,
}

thread_local! {
    static HOOKED: RefCell<Option<Hooked>> = const { RefCell::new(None) };
}

struct KeyTap {
    thread: u32,
    join: Option<JoinHandle<()>>,
}

impl KeyTap {
    fn install(hooked: Hooked) -> Option<Self> {
        let (ready_tx, ready) = mpsc::sync_channel(1);
        let join = thread::Builder::new()
            .name("notchbase dictation key".into())
            .spawn(move || hook_thread(hooked, &ready_tx))
            .ok()?;
        match ready.recv() {
            Ok(Some(thread)) => Some(Self { thread, join: Some(join) }),
            _ => {
                let _ = join.join();
                None
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
    HOOKED.with_borrow(|hooked| {
        if let Some(hooked) = hooked.as_ref().filter(|hooked| hooked.chord.active) {
            (hooked.keys)(Key::Up);
        }
    });
    HOOKED.set(None);
}

fn mask() {
    let strokes = [key(MASK_KEY, 0, 0), key(MASK_KEY, 0, KEYEVENTF_KEYUP)];
    unsafe { SendInput(strokes.len() as u32, strokes.as_ptr(), size_of::<INPUT>() as i32) };
}

unsafe extern "system" fn keyboard_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let info = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
        let message = wparam as u32;
        let down = matches!(message, WM_KEYDOWN | WM_SYSKEYDOWN);
        let up = matches!(message, WM_KEYUP | WM_SYSKEYUP);
        if info.dwExtraInfo != OURS && (down || up) {
            let seen = HOOKED.with(|cell| {
                let Ok(mut hooked) = cell.try_borrow_mut() else { return None };
                let hooked = hooked.as_mut()?;
                let seen = hooked.chord.see(info.vkCode, down, hooked.listening.load(Ordering::Relaxed));
                Some((seen, hooked.keys.clone()))
            });
            match seen {
                Some((Seen::Swallow(key), keys)) => {
                    keys(key);
                    return TAKEN;
                }
                Some((Seen::Masked(key), keys)) => {
                    mask();
                    if let Some(key) = key {
                        keys(key);
                    }
                }
                Some((Seen::Pass(Some(key)), keys)) => keys(key),
                _ => {}
            }
        }
    }
    unsafe { CallNextHookEx(null_mut(), code, wparam, lparam) }
}

struct Microphone {
    running: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl Recording for Microphone {}

impl Microphone {
    fn start(tape: Tape) -> Result<Self, String> {
        let running = Arc::new(AtomicBool::new(true));
        let (ready_tx, ready) = mpsc::sync_channel(1);
        let still = running.clone();
        let join = thread::Builder::new()
            .name("notchbase dictation microphone".into())
            .spawn(move || record(&tape, &still, &ready_tx))
            .map_err(|err| err.to_string())?;
        match ready.recv() {
            Ok(Ok(())) => Ok(Self { running, join: Some(join) }),
            Ok(Err(err)) => {
                let _ = join.join();
                Err(err)
            }
            Err(_) => Err("Couldn't start the microphone".into()),
        }
    }
}

impl Drop for Microphone {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Release);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn record(tape: &Tape, running: &AtomicBool, ready: &SyncSender<Result<(), String>>) {
    let _apartment = Apartment::join();
    let mut audio = match Audio::microphone(None).and_then(|mut audio| audio.start(0).map(|()| audio)) {
        Ok(audio) => audio,
        Err(err) => {
            let _ = ready.send(Err(err));
            return;
        }
    };
    let _ = ready.send(Ok(()));
    let mut converter = Converter::new(DEVICE_RATE, CHANNELS);
    let mut samples = Vec::new();
    let mut out = Vec::new();
    while running.load(Ordering::Acquire) {
        loop {
            match audio.next_packet() {
                Ok(Some(packet)) => {
                    samples.clear();
                    samples.extend(packet.pcm.chunks_exact(2).map(|pair| i16::from_le_bytes([pair[0], pair[1]])));
                    out.clear();
                    converter.push(&samples, &mut out);
                    tape.push(&out);
                    tape.set_level(level(&out));
                }
                Ok(None) => break,
                Err(_) => return,
            }
        }
        thread::sleep(POLL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn play(chord: &mut Chord, strokes: &[(u32, bool)]) -> Vec<Seen> {
        strokes.iter().map(|&(code, down)| chord.see(code, down, false)).collect()
    }

    #[test]
    fn ctrl_win_is_a_chord_either_way_round() {
        let mut chord = Chord::new(Hotkey::CtrlWin);
        let seen = play(&mut chord, &[(LEFT_CTRL, true), (LEFT_WIN, true), (LEFT_WIN, false), (LEFT_CTRL, false)]);
        assert_eq!(
            seen,
            [Seen::Pass(None), Seen::Pass(Some(Key::Down)), Seen::Masked(Some(Key::Up)), Seen::Pass(None)]
        );
        let seen = play(&mut chord, &[(RIGHT_WIN, true), (RIGHT_CTRL, true), (RIGHT_CTRL, false), (RIGHT_WIN, false)]);
        assert_eq!(
            seen,
            [Seen::Pass(None), Seen::Pass(Some(Key::Down)), Seen::Pass(Some(Key::Up)), Seen::Masked(None)]
        );
    }

    #[test]
    fn other_keys_in_the_chord_are_shortcuts() {
        let mut chord = Chord::new(Hotkey::CtrlWin);
        play(&mut chord, &[(LEFT_CTRL, true), (LEFT_WIN, true)]);
        assert_eq!(chord.see(0x44, true, false), Seen::Pass(Some(Key::Other)));
        assert_eq!(Chord::new(Hotkey::CtrlWin).see(0x44, true, false), Seen::Pass(None), "plain typing is ignored");
    }

    #[test]
    fn plain_windows_key_is_left_alone() {
        let mut chord = Chord::new(Hotkey::CtrlWin);
        assert_eq!(play(&mut chord, &[(LEFT_WIN, true), (LEFT_WIN, false)]), [Seen::Pass(None), Seen::Pass(None)]);
    }

    #[test]
    fn right_ctrl_alone_and_escape() {
        let mut chord = Chord::new(Hotkey::RightCtrl);
        assert_eq!(chord.see(RIGHT_CTRL, true, false), Seen::Pass(Some(Key::Down)));
        assert_eq!(chord.see(LEFT_CTRL, true, false), Seen::Pass(Some(Key::Other)));
        assert_eq!(chord.see(ESCAPE, true, true), Seen::Swallow(Key::Escape));
        assert_eq!(chord.see(RIGHT_CTRL, false, true), Seen::Pass(Some(Key::Up)));
        let mut alt = Chord::new(Hotkey::RightAlt);
        assert_eq!(
            play(&mut alt, &[(RIGHT_ALT, true), (RIGHT_ALT, false)]),
            [Seen::Pass(Some(Key::Down)), Seen::Masked(Some(Key::Up))]
        );
    }

    #[test]
    fn keystrokes_are_unicode_and_marked() {
        let inputs = keystrokes("é😀\n");
        assert_eq!(inputs.len(), 2 + 4 + 2);
        let first = unsafe { inputs[0].Anonymous.ki };
        assert_eq!((first.wScan, first.dwFlags, first.dwExtraInfo), (0xE9, KEYEVENTF_UNICODE, OURS));
        let enter = unsafe { inputs[6].Anonymous.ki };
        assert_eq!(enter.wVk, VK_RETURN);
    }
}
