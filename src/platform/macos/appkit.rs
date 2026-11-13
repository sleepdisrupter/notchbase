//! Finds the NSScreen for a monitor and the NSWindow behind a winit window.

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{NSScreen, NSView, NSWindow};
use objc2_foundation::{NSNumber, ns_string};
use raw_window_handle::{HasWindowHandle as _, RawWindowHandle};
use winit::monitor::MonitorHandle;
use winit::platform::macos::MonitorHandleExtMacOS as _;
use winit::window::Window;

pub fn ns_screen(monitor: &MonitorHandle) -> Option<Retained<NSScreen>> {
    let mtm = MainThreadMarker::new()?;
    let display_id = monitor.native_id();
    let key = ns_string!("NSScreenNumber");
    NSScreen::screens(mtm).iter().find(|screen| {
        screen
            .deviceDescription()
            .objectForKey(key)
            .and_then(|value| value.downcast::<NSNumber>().ok())
            .is_some_and(|number| number.unsignedIntValue() == display_id)
    })
}

pub fn ns_window(window: &Window) -> Option<Retained<NSWindow>> {
    MainThreadMarker::new()?;
    let RawWindowHandle::AppKit(handle) = window.window_handle().ok()?.as_raw() else {
        return None;
    };
    let view: &NSView = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
    view.window()
}

pub fn window_origin(window: &Window) -> Option<egui::Pos2> {
    let mtm = MainThreadMarker::new()?;
    let frame = ns_window(window)?.frame();
    let primary = NSScreen::screens(mtm).firstObject()?.frame();
    let top = primary.origin.y + primary.size.height - frame.origin.y - frame.size.height;
    Some(egui::pos2(frame.origin.x as f32, top as f32))
}
