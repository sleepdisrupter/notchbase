//! The color sampler. macOS shows the system's own sampler, and Windows draws a loupe of ours around the cursor.

#[cfg(target_os = "windows")]
mod loupe;

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

use egui::Color32;

pub use imp::Sampler;

pub enum SampleOutcome {
    Picked(Color32),
    Cancelled,
}
