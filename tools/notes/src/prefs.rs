//! The notes tool's settings.

use std::path::PathBuf;

use notchbase_sdk::prelude::*;
use notchbase_sdk::protocol::Dirs;

pub const LIMIT: &str = "limit";
pub const EXPORT: &str = "export";
pub const FOLDER: &str = "folder";
pub const DETAILS: &str = "details";
const DEFAULT_FOLDER: &str = "Notes";

#[derive(Clone, Debug, PartialEq)]
pub struct Prefs {
    pub limit: usize,
    pub export: bool,
    pub folder: Option<PathBuf>,
    pub details: bool,
    pub dirs: Dirs,
}

impl Prefs {
    pub fn from(settings: &Settings, dirs: &Dirs) -> Self {
        Self {
            limit: settings.number(LIMIT).clamp(5.0, 100.0) as usize,
            export: settings.bool(EXPORT),
            folder: settings.path(FOLDER),
            details: settings.bool(DETAILS),
            dirs: dirs.clone(),
        }
    }

    pub fn exports(&self) -> bool {
        self.export_folder().is_some()
    }

    pub fn export_folder(&self) -> Option<PathBuf> {
        if !self.export {
            return None;
        }
        let documents = || self.dirs.documents.as_ref().map(|documents| PathBuf::from(documents).join(DEFAULT_FOLDER));
        self.folder.clone().or_else(documents)
    }
}
