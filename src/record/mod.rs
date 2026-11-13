//! Screen recording: a display or part of one to a video file, with the microphone and desktop audio if asked.
//! Windows has a pipeline of its own here, and macOS uses ScreenCaptureKit.

mod job;
mod naming;
mod options;
mod recorder;
mod region;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as imp;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
use windows as imp;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod other;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use other as imp;

pub use naming::target as unique_file;
pub use options::Options;
pub use recorder::{Recorder, Saved};
pub use region::{Display, Region};

#[cfg(target_os = "windows")]
pub use imp::Loopback;
