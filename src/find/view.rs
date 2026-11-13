//! What the Find page reads (the state, the rows, the pins and the status) and what a pick says back.

use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub use super::kind::Kind;

const PANE_ENDING: &str = "appex";
pub const SHOWN: usize = 20;
pub const ONE_SCREEN: usize = 4;
pub const PINNED_AT_MOST: usize = 4;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FindView {
    pub state: FindState,
    pub answered: u64,
    pub query: Arc<str>,
    pub rows: Arc<[FindRow]>,
    pub reading: Option<Arc<str>>,
    pub more: bool,
    pub indexing: Option<u32>,
    pub entries: u32,
    pub access: Access,
    pub nudge: bool,
    pub truncated: bool,
    pub gone: Arc<[u64]>,
    pub system_key: SystemKey,
    pub pins: Arc<[FindRow]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pinned {
    pub path: PathBuf,
    pub name: String,
    pub app: bool,
    pub folder: bool,
}

impl Pinned {
    pub fn of(row: &FindRow) -> Self {
        Self { path: row.path.to_path_buf(), name: row.name.to_string(), app: row.app, folder: row.folder }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SystemKey {
    #[default]
    Off,
    On,
    Blocked,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FindState {
    #[default]
    Off,
    Starting,
    Ready,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Access {
    #[default]
    Full,
    Limited,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FindRow {
    pub key: u64,
    pub path: Arc<Path>,
    pub name: Arc<str>,
    pub place: Arc<str>,
    pub marks: Vec<Range<u16>>,
    pub kind: Kind,
    pub app: bool,
    pub folder: bool,
    pub cloud: bool,
    pub modified: u32,
    pub why: Option<Why>,
    pub predicted: bool,
    pub icon: Option<Arc<egui::ColorImage>>,
}

impl FindRow {
    pub fn pane(&self) -> bool {
        self.app && self.path.extension().is_some_and(|ending| ending.eq_ignore_ascii_case(PANE_ENDING))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Why {
    Opened(u32),
    Edited(u32),
    Added(u32),
    After(Arc<str>),
    AroundNow,
    With(Arc<str>),
    Busy(u16),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pick {
    pub path: PathBuf,
    pub typed: String,
    pub rank: u8,
    pub how: How,
    pub predicted: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum How {
    Open,
    /// Opened as administrator, behind the UAC prompt. Only Windows asks for it, and only for apps.
    AsAdmin,
    Reveal,
    Drag,
    Shelve,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Answer {
    pub generation: u64,
    pub query: Arc<str>,
    pub rows: Vec<FindRow>,
    pub reading: Option<Arc<str>>,
    pub more: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Status {
    pub phase: Phase,
    pub entries: u32,
    pub access: Access,
    pub truncated: bool,
    pub unwatched: u8,
}

impl Default for Status {
    fn default() -> Self {
        Self { phase: Phase::Off, entries: 0, access: Access::Limited, truncated: false, unwatched: 0 }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    #[default]
    Off,
    Loading,
    Scanning,
    Watching,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_view_is_off() {
        let view = FindView::default();
        assert_eq!((view.state, view.access), (FindState::Off, Access::Full));
        assert!(view.rows.is_empty() && view.query.is_empty() && !view.nudge && view.indexing.is_none());
        assert_eq!(view.system_key, SystemKey::Off, "the system's search key is left alone until asked");
        assert_eq!(Status::default().phase, Phase::Off);
        assert_eq!(Status::default().access, Access::Limited, "access nobody has probed is not assumed");
        assert_eq!(Answer::default().rows.len(), 0);
    }
}
