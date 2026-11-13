//! The system calendar: events around now for the Calendar page, and saving or deleting them. Apple Calendar only.

mod change;
mod watch;
mod window;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use self::macos as imp;

#[cfg(not(target_os = "macos"))]
mod other;
#[cfg(not(target_os = "macos"))]
use self::other as imp;

pub use change::Change;
pub use watch::SystemCalendar;
