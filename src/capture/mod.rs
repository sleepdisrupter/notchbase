//! Screen capture: the color sampler, the screenshot and recording area pickers, and the dim around a recording.

pub mod color;
mod dim;
mod overlay;
pub mod region;
mod screenshot;

pub use dim::RecordingDim;
pub use screenshot::Screenshot;

/// Covers a monitor with an overlay window, for overlays outside capture like the snap ghost.
pub fn fit_overlay_to_monitor(window: &winit::window::Window, monitor: &winit::monitor::MonitorHandle) {
    overlay::fit::fit_to_monitor(window, monitor);
}

/// Which picker a capture request opens: the color sampler, a screenshot area, or an area to record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureMode {
    Color,
    Region,
    Area,
}
