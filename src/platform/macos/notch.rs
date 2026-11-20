//! Measures a screen's hardware notch from its safe area and the menu bar strips on either side of the camera.

use objc2::runtime::NSObjectProtocol as _;
use objc2::sel;
use winit::monitor::MonitorHandle;

use super::appkit::ns_screen;

const MAX_NOTCH_SHARE_OF_WIDTH: f64 = 0.5;

pub fn hardware_notch(monitor: &MonitorHandle) -> Option<egui::Vec2> {
    let screen = ns_screen(monitor)?;
    if !screen.respondsToSelector(sel!(safeAreaInsets)) || !screen.respondsToSelector(sel!(auxiliaryTopLeftArea)) {
        return None;
    }
    let top = screen.safeAreaInsets().top;
    if top <= 0.0 {
        return None;
    }
    let left = screen.auxiliaryTopLeftArea().size.width;
    let right = screen.auxiliaryTopRightArea().size.width;
    let screen_width = screen.frame().size.width;
    if left <= 0.0 || right <= 0.0 {
        return None;
    }
    let width = screen_width - left - right;
    (width > 0.0 && width < screen_width * MAX_NOTCH_SHARE_OF_WIDTH).then(|| egui::vec2(width as f32, top as f32))
}

/// Whether `monitor` is a virtual display Roam made for another computer's screen (vendor "RO", as Roam names it). Its
/// pictures show on that computer, where a notch would sit over its own and catch the clicks made there.
pub fn virtual_display(monitor: &MonitorHandle) -> bool {
    use winit::platform::macos::MonitorHandleExtMacOS as _;
    const ROAM_VENDOR: u32 = 0x524F;
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGDisplayVendorNumber(display: u32) -> u32;
    }
    // SAFETY: a plain query of a display the system listed.
    unsafe { CGDisplayVendorNumber(monitor.native_id()) == ROAM_VENDOR }
}
