//! The volume and media keys on Windows, through a low-level keyboard hook.

use std::cell::RefCell;
use std::ptr::{null, null_mut};
use std::sync::Arc;
use std::sync::mpsc::{self, Sender, SyncSender};
use std::thread::JoinHandle;

use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    VK_MEDIA_NEXT_TRACK, VK_MEDIA_PLAY_PAUSE, VK_MEDIA_PREV_TRACK, VK_VOLUME_DOWN, VK_VOLUME_MUTE, VK_VOLUME_UP,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, HC_ACTION, KBDLLHOOKSTRUCT, MSG, PM_NOREMOVE, PeekMessageW, PostThreadMessageW,
    SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL, WM_KEYDOWN, WM_QUIT, WM_SYSKEYDOWN,
};

use crate::system::volume::tap::{TapState, media, volume};
use crate::system::volume::{Key, MediaKey, Press};

const NO_HOOK: &str = "couldn't watch the volume keys";
const TAKEN: LRESULT = 1;

pub struct KeyTap {
    thread: u32,
    join: Option<JoinHandle<()>>,
}

impl KeyTap {
    pub fn install(keys: Sender<Key>, state: Arc<TapState>) -> Result<Self, String> {
        let (ready_tx, ready) = mpsc::sync_channel(1);
        let join = std::thread::Builder::new()
            .name("notchbase keys".into())
            .spawn(move || hook_thread(Hooked { keys, state, held: Vec::new() }, &ready_tx))
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
    keys: Sender<Key>,
    state: Arc<TapState>,
    held: Vec<u32>,
}

impl Hooked {
    fn wanted(&self, code: u32) -> Option<Key> {
        self.state.wants(key_for(code))
    }

    fn take(&mut self, code: u32, key: Key, down: bool) {
        let repeat = self.held.contains(&code);
        match (down, repeat) {
            (true, false) => self.held.push(code),
            (false, _) => self.held.retain(|&held| held != code),
            (true, true) => {}
        }
        if down && (!repeat || matches!(key, Key::Volume(_))) {
            let _ = self.keys.send(key);
        }
    }
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
}

fn key_for(code: u32) -> Option<Key> {
    Some(match u16::try_from(code).ok()? {
        VK_VOLUME_UP => volume(Press::Up, false),
        VK_VOLUME_DOWN => volume(Press::Down, false),
        VK_VOLUME_MUTE => volume(Press::Mute, false),
        VK_MEDIA_PLAY_PAUSE => media(MediaKey::PlayPause),
        VK_MEDIA_NEXT_TRACK => media(MediaKey::Next),
        VK_MEDIA_PREV_TRACK => media(MediaKey::Previous),
        _ => return None,
    })
}

fn with_hooked<T>(act: impl FnOnce(&mut Hooked) -> T) -> Option<T> {
    HOOKED.with(|cell| cell.try_borrow_mut().ok().and_then(|mut hooked| hooked.as_mut().map(act)))
}

fn settle(downstream: LRESULT, wanted: Option<Key>) -> Option<Key> {
    if downstream == 0 { wanted } else { None }
}

unsafe extern "system" fn keyboard_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let downstream = unsafe { CallNextHookEx(null_mut(), code, wparam, lparam) };
    if code != HC_ACTION as i32 {
        return downstream;
    }
    let info = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
    let wanted = with_hooked(|hooked| hooked.wanted(info.vkCode)).flatten();
    let Some(key) = settle(downstream, wanted) else { return downstream };
    let down = matches!(wparam as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
    with_hooked(|hooked| hooked.take(info.vkCode, key, down));
    TAKEN
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hooked() -> (Hooked, mpsc::Receiver<Key>) {
        let (keys, received) = mpsc::channel();
        let state = Arc::new(TapState::default());
        state.claim_volume(true);
        state.claim_media(true);
        (Hooked { keys, state, held: Vec::new() }, received)
    }

    #[test]
    fn volume_and_media_keys_wanted() {
        assert_eq!(key_for(u32::from(VK_VOLUME_UP)), Some(volume(Press::Up, false)));
        assert_eq!(key_for(u32::from(VK_MEDIA_PLAY_PAUSE)), Some(media(MediaKey::PlayPause)));
        assert_eq!(key_for(0x41), None);
    }

    #[test]
    fn remapped_keys_are_left_alone() {
        let mute = Some(volume(Press::Mute, false));
        assert_eq!(settle(0, mute), mute, "nobody else wanted it, so it's ours");
        assert_eq!(settle(1, mute), None, "a remapper swallowed it, so its replacement is what counts");
        assert_eq!(settle(0, None), None);
    }

    #[test]
    fn held_media_keys_act_once() {
        let (mut hooked, received) = hooked();
        let (up, next) = (u32::from(VK_VOLUME_UP), u32::from(VK_MEDIA_NEXT_TRACK));
        let (up_key, next_key) = (hooked.wanted(up).expect("wanted"), hooked.wanted(next).expect("wanted"));
        for _ in 0..3 {
            hooked.take(up, up_key, true);
            hooked.take(next, next_key, true);
        }
        hooked.take(up, up_key, false);
        hooked.take(next, next_key, false);
        let got: Vec<Key> = received.try_iter().collect();
        assert_eq!(got.iter().filter(|key| matches!(key, Key::Volume(_))).count(), 3);
        assert_eq!(got.iter().filter(|key| **key == media(MediaKey::Next)).count(), 1);
        assert!(hooked.held.is_empty());
    }
}
