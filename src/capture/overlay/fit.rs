//! Places an overlay window exactly over a monitor. On Windows the window is one pixel taller than the monitor.

use winit::dpi::PhysicalSize;
use winit::monitor::MonitorHandle;
use winit::window::Window;

const OVERHANG: u32 = if cfg!(target_os = "windows") { 1 } else { 0 };

pub(in crate::capture) fn fit_to_monitor(window: &Window, monitor: &MonitorHandle) {
    let size = monitor.size();
    let size = PhysicalSize::new(size.width, size.height + OVERHANG);
    if window.outer_position().ok() != Some(monitor.position()) {
        window.set_outer_position(monitor.position());
    }
    if window.inner_size() != size {
        let _ = window.request_inner_size(size);
    }
}
