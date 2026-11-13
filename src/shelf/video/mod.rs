//! Video through the system: probing size and length, transcoding to MP4, MOV, M4A or MP3, and reading frames for GIFs.
//! AVFoundation does it on macOS and Media Foundation on Windows.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod other;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "macos")]
use macos as imp;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use other as imp;
#[cfg(target_os = "windows")]
use windows as imp;

use std::path::Path;

use super::Task;

const EXTENSIONS: [&str; 12] = ["mp4", "m4v", "mov", "avi", "wmv", "mkv", "webm", "mpg", "mpeg", "3gp", "mts", "m2ts"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Container {
    Mp4,
    Mov,
    M4a,
    Mp3,
}

impl Container {
    pub(super) fn is_audio_only(self) -> bool {
        matches!(self, Container::M4a | Container::Mp3)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Quality {
    Original,
    Compact,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Probe {
    pub(super) dims: Option<[u32; 2]>,
    pub(super) seconds: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct FrameSpec {
    pub(super) max_side: u32,
    pub(super) fps: f64,
    pub(super) max_seconds: f64,
}

pub(super) struct Frame {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) rgba: Vec<u8>,
    pub(super) seconds: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MediaError {
    Cancelled,
    Unsupported,
    NoAudio,
    Failed,
}

pub(super) fn is_video_ext(ext: &str) -> bool {
    EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str())
}

pub(super) fn probe(path: &Path) -> Option<Probe> {
    imp::probe(path)
}

pub(super) fn can_write(container: Container) -> bool {
    imp::can_write(container)
}

pub(super) fn transcode(
    source: &Path,
    target: &Path,
    container: Container,
    quality: Quality,
    task: &mut Task,
) -> Result<(), MediaError> {
    if !can_write(container) {
        return Err(MediaError::Unsupported);
    }
    imp::transcode(source, target, container, quality, task)
}

pub(super) fn frames(
    source: &Path,
    spec: FrameSpec,
    task: &mut Task,
    each: &mut dyn FnMut(Frame) -> Result<(), MediaError>,
) -> Result<(), MediaError> {
    imp::frames(source, spec, task, each)
}
