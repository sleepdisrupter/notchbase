//! The notch, overlay and dim windows on Windows. They are tool windows with no taskbar button, and the overlays
//! and the dim stay out of screen captures.

use windows_sys::Win32::UI::WindowsAndMessaging::{
    HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowDisplayAffinity, SetWindowPos,
    WDA_EXCLUDEFROMCAPTURE, WS_EX_APPWINDOW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::monitor::MonitorHandle;
use winit::platform::windows::{CornerPreference, WindowAttributesExtWindows as _};
use winit::window::{Window, WindowAttributes, WindowLevel};

use super::window_style::{hwnd, update_ex_style};
use crate::platform::Placement;

const OVERLAY_IN_CAPTURES: &str = "NOTCHBASE_OVERLAY_IN_CAPTURES";
const OVERLAY_OVERHANG_PX: u32 = 1;

#[derive(Clone, Copy)]
struct Frame {
    position: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
}

impl Frame {
    fn top_center(monitor: &MonitorHandle, canvas: egui::Vec2) -> Self {
        Self::top_center_in(monitor.position(), monitor.size(), monitor.scale_factor(), canvas)
    }

    fn top_center_in(origin: PhysicalPosition<i32>, area: PhysicalSize<u32>, scale: f64, canvas: egui::Vec2) -> Self {
        let size = PhysicalSize::new(physical(canvas.x, scale), physical(canvas.y, scale));
        let x = origin.x + (area.width as i32 - size.width as i32) / 2;
        Self { position: PhysicalPosition::new(x, origin.y), size }
    }

    fn apply_to(self, window: &Window) {
        if window.outer_position().ok() != Some(self.position) {
            window.set_outer_position(self.position);
        }
        if window.inner_size() != self.size {
            let _ = window.request_inner_size(self.size);
        }
    }
}

pub fn notch_window_attributes(placement: &Placement, canvas: egui::Vec2) -> WindowAttributes {
    let frame = Frame::top_center(&placement.monitor, canvas);
    base_attributes("notchbase")
        .with_active(false)
        .with_drag_and_drop(false)
        .with_inner_size(frame.size)
        .with_position(frame.position)
}

pub fn configure_notch_window(window: &Window, placement: &Placement, canvas: egui::Vec2) {
    update_ex_style(window, WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED, WS_EX_APPWINDOW);
    Frame::top_center(&placement.monitor, canvas).apply_to(window);
}

pub fn overlay_window_attributes(monitor: &MonitorHandle) -> WindowAttributes {
    let size = monitor.size();
    base_attributes("notchbase picker")
        .with_inner_size(PhysicalSize::new(size.width, size.height + OVERLAY_OVERHANG_PX))
        .with_position(monitor.position())
}

pub fn configure_overlay_window(window: &Window, _monitor: &MonitorHandle) {
    update_ex_style(window, WS_EX_TOOLWINDOW, WS_EX_APPWINDOW);
    keep_out_of_captures(window);
}

pub fn dim_window_attributes(monitor: &MonitorHandle) -> WindowAttributes {
    overlay_window_attributes(monitor).with_title("notchbase recording")
}

pub fn configure_dim_window(window: &Window, _monitor: &MonitorHandle) {
    update_ex_style(window, WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE, WS_EX_APPWINDOW);
    keep_out_of_captures(window);
}

pub fn raise_window(window: &Window) {
    if let Some(hwnd) = hwnd(window) {
        unsafe { SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE) };
    }
}

pub fn keep_on_top(window: &Window) {
    raise_window(window);
}

fn keep_out_of_captures(window: &Window) {
    if std::env::var_os(OVERLAY_IN_CAPTURES).is_none()
        && let Some(hwnd) = hwnd(window)
    {
        unsafe { SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE) };
    }
}

fn physical(logical: f32, scale: f64) -> u32 {
    (f64::from(logical) * scale).round() as u32
}

fn base_attributes(title: &str) -> WindowAttributes {
    WindowAttributes::default()
        .with_title(title)
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(false)
        .with_visible(false)
        .with_window_level(WindowLevel::AlwaysOnTop)
        .with_skip_taskbar(true)
        .with_undecorated_shadow(false)
        .with_corner_preference(CornerPreference::DoNotRound)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centered_in_physical_pixels() {
        let frame = Frame::top_center_in(
            PhysicalPosition::new(-1920, 0),
            PhysicalSize::new(1920, 1080),
            1.5,
            egui::vec2(400.0, 100.0),
        );
        assert_eq!(frame.size, PhysicalSize::new(600, 150));
        assert_eq!(frame.position, PhysicalPosition::new(-1920 + 660, 0));
    }

    #[test]
    fn wide_canvas_still_centers() {
        let frame = Frame::top_center_in(
            PhysicalPosition::new(0, 0),
            PhysicalSize::new(300, 200),
            1.0,
            egui::vec2(500.0, 10.0),
        );
        assert_eq!(frame.position.x, -100);
    }
}
