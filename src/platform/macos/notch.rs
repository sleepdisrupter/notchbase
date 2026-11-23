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
