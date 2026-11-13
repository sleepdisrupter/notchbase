//! Reading and setting the built-in display's brightness, for the brightness keys. Only macOS for now.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use self::macos as imp;

#[cfg(not(target_os = "macos"))]
mod other;
#[cfg(not(target_os = "macos"))]
use self::other as imp;

pub use imp::Display;

pub const SUPPORTED: bool = cfg!(target_os = "macos");
