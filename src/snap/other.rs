//! Other systems have no window layouts.

use egui::{Pos2, Rect};
use winit::monitor::MonitorHandle;

use super::{Grab, Handle, Screen};

const UNSUPPORTED: &str = "Window layouts need macOS or Windows";

pub(super) fn allowed() -> bool {
    false
}

pub(super) fn ask_for_access() {}

pub(super) fn cursor() -> Option<Pos2> {
    None
}

pub(super) fn screens() -> Vec<Screen> {
    Vec::new()
}

pub(super) fn monitor_key(_: &MonitorHandle) -> u64 {
    0
}

pub(super) fn window_at(_: Pos2) -> Option<Grab> {
    None
}

pub(super) fn frame_of(_: Handle) -> Option<Rect> {
    None
}

pub(super) fn windows_on(_: &Screen) -> Vec<Grab> {
    Vec::new()
}

pub(super) fn place(_: Handle, _: Rect) -> Result<(), String> {
    Err(UNSUPPORTED.into())
}
