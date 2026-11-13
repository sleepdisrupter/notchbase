//! Files in and out of the notch: dragging shelf files out to other apps, taking drops from anywhere, thumbnails,
//! icons, and showing a file in Finder or Explorer.

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod fit;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as backend;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
use windows as backend;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod other;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use other as backend;

mod incoming;
mod links;
mod save;
#[cfg(test)]
mod tests;

pub use incoming::{Arrival, Dropped, Inbox};
pub use save::{is_temporary, save};

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use winit::window::Window;

const LARGEST_THUMBNAIL_PX: u32 = 1024;

static CATCHES_EVERYTHING: AtomicBool = AtomicBool::new(false);

/// While on, the notch takes a drop it can't read, so a tab torn out of Firefox stays where it was.
pub fn catch_everything(on: bool) {
    CATCHES_EVERYTHING.store(on, Ordering::Relaxed);
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn catches_everything() -> bool {
    CATCHES_EVERYTHING.load(Ordering::Relaxed)
}

static PREFERRED_PAGES: OnceLock<fn(&str) -> bool> = OnceLock::new();

/// Names the links that count as a page even beside a picture, like a known video page. Set once at start.
pub fn prefer_pages(wanted: fn(&str) -> bool) {
    let _ = PREFERRED_PAGES.set(wanted);
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn prefers_page(link: &str) -> bool {
    PREFERRED_PAGES.get().is_some_and(|wanted| wanted(link))
}

/// Drags shelf files out to other apps. The button must still be down, or there's no drag to start from.
pub fn start_drag(window: &Window, paths: &[PathBuf]) -> Result<(), String> {
    if paths.is_empty() {
        return Err("Nothing to drag".into());
    }
    if !primary_button_down() {
        return Err("The mouse button was already released".into());
    }
    backend::start_drag(window, paths)
}

pub struct DropSite {
    _site: backend::Site,
}

pub fn accept_drops(window: &Window, inbox: &Inbox) -> Option<DropSite> {
    backend::accept_drops(window, inbox.clone()).map(|site| DropSite { _site: site })
}

/// Whether one of our own drags is under way.
pub fn dragging_out() -> bool {
    backend::dragging_out()
}

pub fn primary_button_down() -> bool {
    backend::primary_button_down()
}

/// What the drag sensor sees while the button is down. Only macOS can tell that files are being dragged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sensed {
    Idle,
    Pressed,
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    Files(usize),
}

#[derive(Default)]
pub struct DragSensor {
    backend: backend::Sensor,
}

impl DragSensor {
    pub fn poll(&mut self) -> Sensed {
        self.backend.poll()
    }
}

/// A picture of the file from the system's thumbnailer, at most 1024 px.
pub fn thumbnail(path: &Path, max_px: u32) -> Option<egui::ColorImage> {
    if max_px == 0 || !path.exists() {
        return None;
    }
    backend::thumbnail(path, max_px.min(LARGEST_THUMBNAIL_PX))
}

pub fn icon(path: &Path, px: u32) -> Option<egui::ColorImage> {
    if px == 0 {
        return None;
    }
    backend::icon(path, px.min(LARGEST_THUMBNAIL_PX))
}

pub fn reveal(path: &Path) {
    backend::reveal(path);
}
