//! What the tool asked for: which audio to record, which microphone, where to save, and a frame rate cap.

use std::path::PathBuf;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Options {
    pub microphone: bool,
    pub desktop: bool,
    pub microphone_id: Option<String>,
    pub folder: Option<PathBuf>,
    pub max_fps: Option<u32>,
}
