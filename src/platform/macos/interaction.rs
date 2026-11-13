//! Click-through, keyboard focus, held keys and buttons, and the cursor, which is polled because a window that
//! ignores the mouse gets no mouse events.

use std::ptr::NonNull;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::{NSEvent, NSEventMask, NSEventModifierFlags};
use winit::window::Window;

use super::appkit::ns_window;

pub struct PressesElsewhere {
    monitor: Retained<AnyObject>,
    pressed: Arc<AtomicBool>,
}

impl PressesElsewhere {
    pub fn take_press(&self) -> bool {
        self.pressed.swap(false, Ordering::Relaxed)
    }
}

impl Drop for PressesElsewhere {
    fn drop(&mut self) {
        unsafe { NSEvent::removeMonitor(&self.monitor) };
    }
}

/// Hears clicks that go to other apps, like the menu bar or the Dock, which move no focus.
pub fn watch_presses_elsewhere() -> Option<PressesElsewhere> {
    let pressed = Arc::new(AtomicBool::new(false));
    let heard = Arc::clone(&pressed);
    let handler = RcBlock::new(move |_: NonNull<NSEvent>| heard.store(true, Ordering::Relaxed));
    let presses = NSEventMask::LeftMouseDown | NSEventMask::RightMouseDown | NSEventMask::OtherMouseDown;
    let monitor = NSEvent::addGlobalMonitorForEventsMatchingMask_handler(presses, &handler)?;
    Some(PressesElsewhere { monitor, pressed })
}

pub fn set_click_through(window: &Window, click_through: bool) {
    let _ = window.set_cursor_hittest(!click_through);
}

pub fn begin_keyboard_focus(window: &Window) {
    window.focus_window();
}

pub fn end_keyboard_focus(_window: &Window) {}

pub fn held_modifiers() -> egui::Modifiers {
    let flags = NSEvent::modifierFlags_class();
    let command = flags.contains(NSEventModifierFlags::Command);
    egui::Modifiers {
        alt: flags.contains(NSEventModifierFlags::Option),
        ctrl: flags.contains(NSEventModifierFlags::Control),
        shift: flags.contains(NSEventModifierFlags::Shift),
        mac_cmd: command,
        command,
    }
}

const COMBINED_SESSION: i32 = 0;
const LAST_KEY: u16 = 127;
const LAST_BUTTON: u32 = 31;
const PRESSES: [u32; 3] = [1, 3, 25];

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventSourceKeyState(state: i32, key: u16) -> bool;
    fn CGEventSourceButtonState(state: i32, button: u32) -> bool;
    fn CGEventSourceSecondsSinceLastEventType(state: i32, kind: u32) -> f64;
}

pub fn any_key_held() -> bool {
    (0..=LAST_KEY).any(|key| unsafe { CGEventSourceKeyState(COMBINED_SESSION, key) })
}

pub fn mouse_held() -> bool {
    (0..=LAST_BUTTON).any(|button| unsafe { CGEventSourceButtonState(COMBINED_SESSION, button) })
}

pub fn pressed_within(seconds: f64) -> bool {
    PRESSES.iter().any(|press| unsafe { CGEventSourceSecondsSinceLastEventType(COMBINED_SESSION, *press) } < seconds)
}

pub fn cursor_in_window(window: &Window) -> Option<egui::Pos2> {
    let ns_window = ns_window(window)?;
    let mouse = NSEvent::mouseLocation();
    let frame = ns_window.frame();
    Some(egui::pos2((mouse.x - frame.origin.x) as f32, (frame.origin.y + frame.size.height - mouse.y) as f32))
}
