//! The volume, brightness and media keys on macOS, through a Quartz event tap on the system-defined key events.

use std::ffi::c_void;
use std::ptr::{NonNull, null_mut};
use std::sync::Arc;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::mpsc::{self, Sender, SyncSender};
use std::thread::JoinHandle;

use objc2::rc::autoreleasepool;
use objc2_app_kit::{NSEvent, NSEventModifierFlags};
use objc2_core_foundation::{CFMachPort, CFRetained, CFRunLoop, kCFRunLoopCommonModes};
use objc2_core_graphics::{
    CGEvent, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventTapProxy, CGEventType,
};

use super::trust::may_watch_keys;
use crate::system::volume::tap::{TapState, brightness, media, volume};
use crate::system::volume::{Key, MediaKey, Press};

const SYSTEM_DEFINED: u32 = 14;
const AUX_CONTROL_BUTTONS: i16 = 8;
const KEY_DOWN: isize = 0xA;
const SOUND_UP: isize = 0;
const SOUND_DOWN: isize = 1;
const BRIGHTNESS_UP: isize = 2;
const BRIGHTNESS_DOWN: isize = 3;
const MUTE: isize = 7;
const PLAY: isize = 16;
const NEXT: isize = 17;
const PREVIOUS: isize = 18;
const FAST: isize = 19;
const REWIND: isize = 20;
const REPEAT: isize = 0x1;
const NOT_ALLOWED: &str = "Allow notchbase in Accessibility settings to use the volume keys";
const NO_TAP: &str = "couldn't watch the volume keys";

struct Context {
    keys: Sender<Key>,
    state: Arc<TapState>,
    port: AtomicPtr<CFMachPort>,
}

struct Loop(CFRetained<CFRunLoop>);

unsafe impl Send for Loop {}

pub struct KeyTap {
    run_loop: Loop,
    join: Option<JoinHandle<()>>,
}

impl KeyTap {
    pub fn install(keys: Sender<Key>, state: Arc<TapState>) -> Result<Self, String> {
        if !may_watch_keys() {
            return Err(NOT_ALLOWED.into());
        }
        let (ready_tx, ready) = mpsc::sync_channel(1);
        let join = std::thread::Builder::new()
            .name("notchbase volume keys".into())
            .spawn(move || tap_thread(keys, state, &ready_tx))
            .map_err(|err| err.to_string())?;
        match ready.recv() {
            Ok(Some(run_loop)) => Ok(Self { run_loop, join: Some(join) }),
            _ => {
                let _ = join.join();
                Err(NO_TAP.into())
            }
        }
    }
}

impl Drop for KeyTap {
    fn drop(&mut self) {
        self.run_loop.0.stop();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn tap_thread(keys: Sender<Key>, state: Arc<TapState>, ready: &SyncSender<Option<Loop>>) {
    let context = Box::new(Context { keys, state, port: AtomicPtr::new(null_mut()) });
    let info = (&*context as *const Context).cast_mut().cast::<c_void>();
    let port = unsafe {
        CGEvent::tap_create(
            CGEventTapLocation::SessionEventTap,
            CGEventTapPlacement::TailAppendEventTap,
            CGEventTapOptions::Default,
            1 << SYSTEM_DEFINED,
            Some(tapped),
            info,
        )
    };
    let (Some(port), Some(run_loop)) = (port, CFRunLoop::current()) else {
        let _ = ready.send(None);
        return;
    };
    context.port.store((&*port as *const CFMachPort).cast_mut(), Ordering::Release);
    let Some(source) = CFMachPort::new_run_loop_source(None, Some(&port), 0) else {
        let _ = ready.send(None);
        return;
    };
    run_loop.add_source(Some(&source), unsafe { kCFRunLoopCommonModes });
    CGEvent::tap_enable(&port, true);
    let _ = ready.send(Some(Loop(run_loop)));
    CFRunLoop::run();
    CGEvent::tap_enable(&port, false);
    context.port.store(null_mut(), Ordering::Release);
    drop(source);
    drop(port);
    drop(context);
}

fn key_for(code: isize, fine: bool) -> Option<Key> {
    Some(match code {
        SOUND_UP => volume(Press::Up, fine),
        SOUND_DOWN => volume(Press::Down, fine),
        MUTE => volume(Press::Mute, fine),
        BRIGHTNESS_UP => brightness(true, fine),
        BRIGHTNESS_DOWN => brightness(false, fine),
        PLAY => media(MediaKey::PlayPause),
        NEXT | FAST => media(MediaKey::Next),
        PREVIOUS | REWIND => media(MediaKey::Previous),
        _ => return None,
    })
}

unsafe extern "C-unwind" fn tapped(
    _proxy: CGEventTapProxy,
    kind: CGEventType,
    event: NonNull<CGEvent>,
    info: *mut c_void,
) -> *mut CGEvent {
    let pass = event.as_ptr();
    let Some(context) = (unsafe { info.cast::<Context>().as_ref() }) else { return pass };
    if kind == CGEventType::TapDisabledByTimeout || kind == CGEventType::TapDisabledByUserInput {
        if let Some(port) = unsafe { context.port.load(Ordering::Acquire).as_ref() } {
            CGEvent::tap_enable(port, true);
        }
        return pass;
    }
    if kind.0 != SYSTEM_DEFINED {
        return pass;
    }
    let taken = autoreleasepool(|_| {
        let ns = NSEvent::eventWithCGEvent(unsafe { event.as_ref() })?;
        if ns.subtype().0 != AUX_CONTROL_BUTTONS {
            return None;
        }
        let data = ns.data1();
        let code = (data & 0xFFFF_0000) >> 16;
        let fine = ns.modifierFlags().contains(NSEventModifierFlags::Option | NSEventModifierFlags::Shift);
        let key = context.state.wants(key_for(code, fine))?;
        let down = (data & 0xFF00) >> 8 == KEY_DOWN;
        let repeat = data & REPEAT != 0;
        if down && (!repeat || matches!(key, Key::Volume(_) | Key::Brightness(_))) {
            let _ = context.keys.send(key);
        }
        Some(())
    });
    if taken.is_some() { null_mut() } else { pass }
}
