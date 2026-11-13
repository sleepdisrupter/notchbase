//! Downloads in progress, read from the Downloads folder: how far along each one is and when it lands.

mod scan;
mod tracker;
mod watch;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use self::macos as imp;

#[cfg(not(target_os = "macos"))]
mod other;
#[cfg(not(target_os = "macos"))]
use self::other as imp;

pub use watch::Downloads;
