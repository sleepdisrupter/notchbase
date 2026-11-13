//! Command-Space on macOS through a Quartz event tap, which needs Accessibility.

use std::ffi::c_void;
use std::ptr::{NonNull, null_mut};
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Mutex, PoisonError};
use std::thread::JoinHandle;

use objc2_core_foundation::{CFMachPort, CFRetained, CFRunLoop, kCFRunLoopCommonModes};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventFlags, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventTapProxy,
    CGEventType,
};

use super::Pressed;
use super::chord::{Chord, Mods, Stroke, Verdict};
use crate::system::volume::may_watch_keys;

const THREAD: &str = "notchbase summon key";
const NOT_ALLOWED: &str = "Allow notchbase in Accessibility settings to open Find with Command-Space";
const NO_TAP: &str = "couldn't watch for Command-Space";

struct Context {
    pressed: Pressed,
    chord: Mutex<Chord>,
    port: AtomicPtr<CFMachPort>,
}

struct Loop(CFRetained<CFRunLoop>);

unsafe impl Send for Loop {}

pub struct KeyTap {
    run_loop: Loop,
    join: Option<JoinHandle<()>>,
}

impl KeyTap {
    pub fn install(pressed: Pressed) -> Result<Self, String> {
        if !may_watch_keys() {
            return Err(NOT_ALLOWED.into());
        }
        let (ready_tx, ready) = mpsc::sync_channel(1);
        let join = std::thread::Builder::new()
            .name(THREAD.into())
            .spawn(move || tap_thread(pressed, &ready_tx))
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

fn tap_thread(pressed: Pressed, ready: &SyncSender<Option<Loop>>) {
    let context = Box::new(Context { pressed, chord: Mutex::default(), port: AtomicPtr::new(null_mut()) });
    let info = (&*context as *const Context).cast_mut().cast::<c_void>();
    let keys = (1 << CGEventType::KeyDown.0) | (1 << CGEventType::KeyUp.0);
    let port = unsafe {
        CGEvent::tap_create(
            CGEventTapLocation::HIDEventTap,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::Default,
            keys,
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

fn mods(flags: CGEventFlags) -> Mods {
    Mods {
        command: flags.contains(CGEventFlags::MaskCommand),
        shift: flags.contains(CGEventFlags::MaskShift),
        option: flags.contains(CGEventFlags::MaskAlternate),
        control: flags.contains(CGEventFlags::MaskControl),
    }
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
    if kind != CGEventType::KeyDown && kind != CGEventType::KeyUp {
        return pass;
    }
    let event = Some(unsafe { event.as_ref() });
    let stroke = Stroke {
        code: CGEvent::integer_value_field(event, CGEventField::KeyboardEventKeycode),
        down: kind == CGEventType::KeyDown,
        repeat: CGEvent::integer_value_field(event, CGEventField::KeyboardEventAutorepeat) != 0,
        mods: mods(CGEvent::flags(event)),
    };
    let verdict = context.chord.lock().unwrap_or_else(PoisonError::into_inner).see(stroke);
    match verdict {
        Verdict::Pass => pass,
        Verdict::Swallow => null_mut(),
        Verdict::Summon => {
            (context.pressed)();
            null_mut()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_four_modifiers_are_read() {
        let command = CGEventFlags::MaskCommand;
        assert_eq!(mods(command), Mods { command: true, ..Mods::default() });
        assert_eq!(mods(command | CGEventFlags::MaskAlphaShift | CGEventFlags::MaskSecondaryFn), mods(command));
        let all = command | CGEventFlags::MaskShift | CGEventFlags::MaskAlternate | CGEventFlags::MaskControl;
        assert_eq!(mods(all), Mods { command: true, shift: true, option: true, control: true });
        assert_eq!(mods(CGEventFlags::empty()), Mods::default());
    }
}
