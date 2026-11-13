//! Where the front app's menus end, read every 700 ms, so the notch can step aside when they run under it.

mod edge;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use self::macos as imp;

#[cfg(not(target_os = "macos"))]
mod other;
#[cfg(not(target_os = "macos"))]
use self::other as imp;

use std::time::Duration;

use egui::Rect;

use crate::poll::Poll;

pub use edge::edge_in;

const EVERY: Duration = Duration::from_millis(700);

pub struct MenuWatch {
    poll: Poll<Vec<Rect>>,
}

impl MenuWatch {
    pub fn start() -> Self {
        Self { poll: Poll::start("notchbase-menus", EVERY, imp::reader) }
    }

    pub fn take(&self) -> Option<Vec<Rect>> {
        self.poll.take()
    }
}
