//! Low-level mouse and keyboard hooks on their own thread while the loupe is open. They swallow the input the sampler
//! uses, so the click that picks a color doesn't also click the app underneath.

use std::cell::RefCell;
use std::ptr::{null, null_mut};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::thread::JoinHandle;

use windows_sys::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_DOWN, VK_ESCAPE, VK_LEFT, VK_RETURN, VK_RIGHT, VK_SHIFT, VK_SPACE, VK_UP,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, HC_ACTION, HHOOK, HOOKPROC, KBDLLHOOKSTRUCT, MSG, MSLLHOOKSTRUCT, PM_NOREMOVE,
    PeekMessageW, PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL, WHEEL_DELTA,
    WINDOWS_HOOK_ID, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEWHEEL, WM_QUIT, WM_RBUTTONDOWN, WM_RBUTTONUP,
    WM_SYSKEYDOWN,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum HookEvent {
    Pick,
    Cancel,
    Zoom(f32),
    Nudge(i32, i32),
}

impl HookEvent {
    fn repeats(self) -> bool {
        matches!(self, Self::Nudge(..))
    }
}

pub(super) struct Hooks {
    thread: u32,
    events: Receiver<HookEvent>,
    join: Option<JoinHandle<()>>,
}

impl Hooks {
    pub(super) fn install() -> Result<Self, String> {
        let (events_tx, events) = mpsc::channel();
        let (ready_tx, ready) = mpsc::sync_channel(1);
        let join = std::thread::Builder::new()
            .name("notchbase sampler input".into())
            .spawn(move || hook_thread(events_tx, ready_tx))
            .map_err(|e| format!("couldn't start the input thread: {e}"))?;
        match ready.recv() {
            Ok(Ok(thread)) => Ok(Self { thread, events, join: Some(join) }),
            Ok(Err(err)) => {
                let _ = join.join();
                Err(err)
            }
            Err(_) => Err("the input thread stopped unexpectedly".into()),
        }
    }

    pub(super) fn drain(&self) -> Vec<HookEvent> {
        self.events.try_iter().collect()
    }
}

impl Drop for Hooks {
    fn drop(&mut self) {
        unsafe { PostThreadMessageW(self.thread, WM_QUIT, 0, 0) };
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

struct HookThread {
    gate: Gate,
    events: Sender<HookEvent>,
}

thread_local! {
    static HOOK_THREAD: RefCell<Option<HookThread>> = const { RefCell::new(None) };
}

struct Hook(HHOOK);

impl Hook {
    fn install(id: WINDOWS_HOOK_ID, procedure: HOOKPROC, module: HINSTANCE) -> Option<Self> {
        let hook = unsafe { SetWindowsHookExW(id, procedure, module, 0) };
        (!hook.is_null()).then(|| Self(hook))
    }
}

impl Drop for Hook {
    fn drop(&mut self) {
        unsafe { UnhookWindowsHookEx(self.0) };
    }
}

fn hook_thread(events: Sender<HookEvent>, ready: SyncSender<Result<u32, String>>) {
    HOOK_THREAD.set(Some(HookThread { gate: Gate::default(), events }));
    let mut msg = MSG::default();
    // Peeking creates this thread's message queue, so the quit posted on drop can reach it.
    unsafe { PeekMessageW(&mut msg, null_mut(), 0, 0, PM_NOREMOVE) };
    let module = unsafe { GetModuleHandleW(null()) };
    let mouse = Hook::install(WH_MOUSE_LL, Some(mouse_hook), module);
    let keyboard = Hook::install(WH_KEYBOARD_LL, Some(keyboard_hook), module);
    let (Some(_mouse), Some(_keyboard)) = (mouse, keyboard) else {
        let _ = ready.send(Err("couldn't watch the mouse and keyboard".into()));
        return;
    };
    let _ = ready.send(Ok(unsafe { GetCurrentThreadId() }));
    // Low-level hooks are called on the thread that set them, so it has to keep pumping messages.
    while unsafe { GetMessageW(&mut msg, null_mut(), 0, 0) } > 0 {}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Input {
    LeftButton,
    RightButton,
    Key(u16),
}

#[derive(Debug, PartialEq)]
enum Verdict {
    Pass,
    Swallow(Option<HookEvent>),
}

/// Remembers what's held. A release is swallowed only when its press was, so a click or key that
/// started before the loupe opened still ends normally.
#[derive(Default)]
struct Gate {
    held: Vec<Input>,
}

impl Gate {
    fn mouse(&mut self, message: u32, mouse_data: u32) -> Verdict {
        match message {
            WM_LBUTTONDOWN => self.press(Input::LeftButton, None),
            WM_RBUTTONDOWN => self.press(Input::RightButton, None),
            WM_LBUTTONUP => self.release(Input::LeftButton, Some(HookEvent::Pick)),
            WM_RBUTTONUP => self.release(Input::RightButton, Some(HookEvent::Cancel)),
            WM_MOUSEWHEEL => Verdict::Swallow(Some(HookEvent::Zoom(wheel_notches(mouse_data)))),
            _ => Verdict::Pass,
        }
    }

    fn key(&mut self, vk: u16, down: bool, shift: bool) -> Verdict {
        let Some(event) = key_event(vk, shift) else { return Verdict::Pass };
        let input = Input::Key(vk);
        match (down, self.held.contains(&input)) {
            (true, false) => self.press(input, Some(event)),
            (true, true) => Verdict::Swallow(event.repeats().then_some(event)),
            (false, _) => self.release(input, None),
        }
    }

    fn press(&mut self, input: Input, event: Option<HookEvent>) -> Verdict {
        if !self.held.contains(&input) {
            self.held.push(input);
        }
        Verdict::Swallow(event)
    }

    fn release(&mut self, input: Input, event: Option<HookEvent>) -> Verdict {
        let Some(i) = self.held.iter().position(|&held| held == input) else { return Verdict::Pass };
        self.held.swap_remove(i);
        Verdict::Swallow(event)
    }
}

fn key_event(vk: u16, shift: bool) -> Option<HookEvent> {
    let step = if shift { 10 } else { 1 };
    Some(match vk {
        VK_LEFT => HookEvent::Nudge(-step, 0),
        VK_RIGHT => HookEvent::Nudge(step, 0),
        VK_UP => HookEvent::Nudge(0, -step),
        VK_DOWN => HookEvent::Nudge(0, step),
        VK_RETURN | VK_SPACE => HookEvent::Pick,
        VK_ESCAPE => HookEvent::Cancel,
        _ => return None,
    })
}

fn wheel_notches(mouse_data: u32) -> f32 {
    f32::from((mouse_data >> 16) as u16 as i16) / WHEEL_DELTA as f32
}

fn swallows(decide: impl FnOnce(&mut Gate) -> Verdict) -> bool {
    HOOK_THREAD.with(|cell| {
        let Ok(mut state) = cell.try_borrow_mut() else { return false };
        let Some(HookThread { gate, events }) = state.as_mut() else { return false };
        let Verdict::Swallow(event) = decide(gate) else { return false };
        if let Some(event) = event {
            let _ = events.send(event);
        }
        true
    })
}

unsafe extern "system" fn mouse_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let info = unsafe { &*(lparam as *const MSLLHOOKSTRUCT) };
        if swallows(|gate| gate.mouse(wparam as u32, info.mouseData)) {
            return 1;
        }
    }
    unsafe { CallNextHookEx(null_mut(), code, wparam, lparam) }
}

unsafe extern "system" fn keyboard_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let info = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
        let down = matches!(wparam as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
        let shift = unsafe { GetAsyncKeyState(i32::from(VK_SHIFT)) } < 0;
        if swallows(|gate| gate.key(info.vkCode as u16, down, shift)) {
            return 1;
        }
    }
    unsafe { CallNextHookEx(null_mut(), code, wparam, lparam) }
}

#[cfg(test)]
mod tests {
    use windows_sys::Win32::UI::WindowsAndMessaging::{WM_MBUTTONDOWN, WM_MOUSEMOVE};

    use super::*;

    const VK_C: u16 = 0x43;

    #[test]
    fn click_picks_right_click_cancels() {
        let mut gate = Gate::default();
        assert_eq!(gate.mouse(WM_LBUTTONDOWN, 0), Verdict::Swallow(None));
        assert_eq!(gate.mouse(WM_LBUTTONUP, 0), Verdict::Swallow(Some(HookEvent::Pick)));
        assert_eq!(gate.mouse(WM_RBUTTONDOWN, 0), Verdict::Swallow(None));
        assert_eq!(gate.mouse(WM_RBUTTONUP, 0), Verdict::Swallow(Some(HookEvent::Cancel)));
        assert!(gate.held.is_empty());
    }

    #[test]
    fn releases_of_unseen_presses_pass() {
        let mut gate = Gate::default();
        assert_eq!(gate.mouse(WM_LBUTTONUP, 0), Verdict::Pass);
        assert_eq!(gate.mouse(WM_RBUTTONUP, 0), Verdict::Pass);
        assert_eq!(gate.key(VK_ESCAPE, false, false), Verdict::Pass);
        assert_eq!(gate.key(VK_LEFT, false, false), Verdict::Pass);
    }

    #[test]
    fn wheel_zooms_other_mouse_passes() {
        let mut gate = Gate::default();
        assert_eq!(gate.mouse(WM_MOUSEWHEEL, 120 << 16), Verdict::Swallow(Some(HookEvent::Zoom(1.0))));
        let down = u32::from((-240i16) as u16) << 16;
        assert_eq!(gate.mouse(WM_MOUSEWHEEL, down), Verdict::Swallow(Some(HookEvent::Zoom(-2.0))));
        assert_eq!(gate.mouse(WM_MOUSEMOVE, 0), Verdict::Pass);
        assert_eq!(gate.mouse(WM_MBUTTONDOWN, 0), Verdict::Pass);
    }

    #[test]
    fn held_arrows_repeat_keys_act_once() {
        let mut gate = Gate::default();
        for _ in 0..3 {
            assert_eq!(gate.key(VK_LEFT, true, false), Verdict::Swallow(Some(HookEvent::Nudge(-1, 0))));
        }
        assert_eq!(gate.key(VK_DOWN, true, true), Verdict::Swallow(Some(HookEvent::Nudge(0, 10))));
        assert_eq!(gate.key(VK_LEFT, false, false), Verdict::Swallow(None));
        assert_eq!(gate.key(VK_RETURN, true, false), Verdict::Swallow(Some(HookEvent::Pick)));
        assert_eq!(gate.key(VK_RETURN, true, false), Verdict::Swallow(None));
        assert_eq!(gate.key(VK_RETURN, false, false), Verdict::Swallow(None));
        assert_eq!(gate.key(VK_ESCAPE, true, false), Verdict::Swallow(Some(HookEvent::Cancel)));
        assert_eq!(gate.key(VK_ESCAPE, true, false), Verdict::Swallow(None));
        assert_eq!(gate.key(VK_SPACE, true, false), Verdict::Swallow(Some(HookEvent::Pick)));
    }

    #[test]
    fn other_keys_are_left_alone() {
        let mut gate = Gate::default();
        assert_eq!(gate.key(VK_C, true, false), Verdict::Pass);
        assert_eq!(gate.key(VK_C, false, false), Verdict::Pass);
        assert!(gate.held.is_empty());
    }
}
