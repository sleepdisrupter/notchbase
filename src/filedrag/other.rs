//! Other systems have no drags, drops, thumbnails or file icons.

use std::path::{Path, PathBuf};

use winit::window::Window;

use super::Sensed;
use super::incoming::Inbox;

pub(in crate::filedrag) struct Site;

pub(in crate::filedrag) fn accept_drops(_window: &Window, _inbox: Inbox) -> Option<Site> {
    None
}

pub(in crate::filedrag) fn start_drag(_window: &Window, _paths: &[PathBuf]) -> Result<(), String> {
    Err("Dragging files out isn't supported on this platform".into())
}

pub(in crate::filedrag) fn dragging_out() -> bool {
    false
}

pub(in crate::filedrag) fn primary_button_down() -> bool {
    false
}

#[derive(Default)]
pub(in crate::filedrag) struct Sensor;

impl Sensor {
    pub(in crate::filedrag) fn poll(&mut self) -> Sensed {
        Sensed::Idle
    }
}

pub(in crate::filedrag) fn thumbnail(_path: &Path, _max_px: u32) -> Option<egui::ColorImage> {
    None
}

pub(in crate::filedrag) fn icon(_path: &Path, _px: u32) -> Option<egui::ColorImage> {
    None
}

pub(in crate::filedrag) fn reveal(_path: &Path) {}
