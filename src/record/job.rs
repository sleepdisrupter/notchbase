//! What a platform session is asked to record, and what it hands back once the file is saved.

use std::path::PathBuf;

use super::Region;

pub(super) struct Job {
    pub(super) path: PathBuf,
    pub(super) region: Region,
    pub(super) microphone: bool,
    pub(super) desktop: bool,
    pub(super) microphone_id: Option<String>,
    pub(super) max_fps: Option<u32>,
}

/// A finished recording's length, with a note when something went wrong partway.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Take {
    pub(super) seconds: f64,
    pub(super) note: Option<String>,
}
