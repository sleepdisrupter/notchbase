//! Saving and loading the index. It lives in index.bin in the machine's local data folder, beside usage.bin, and only
//! a settled index is written. `Meta` carries the change cursor, the memory level and when each root was last scanned.

mod check;
pub mod frame;
mod index;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod timing;

use std::io;
use std::path::{Path, PathBuf};

use super::model::Live;

pub const FOLDER: &str = "find";
pub const INDEX_FILE: &str = "index.bin";
pub const USAGE_FILE: &str = "usage.bin";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Meta {
    pub saved_at: u32,
    pub cursor: u64,
    pub level: u8,
    pub flags: u8,
    pub roots: Vec<RootMark>,
}

pub mod meta {
    pub const GUARDED_DONE: u8 = 1;
}

#[derive(Clone, Debug, PartialEq)]
pub struct RootMark {
    pub path: PathBuf,
    pub volume: [u8; 16],
    pub flags: u8,
    pub scanned_at: u32,
    pub swept_at: u32,
}

pub mod mark {
    pub const COMPLETE: u8 = 1;
}

pub fn dir() -> Option<PathBuf> {
    crate::platform::local_data_dir().map(|data| data.join(FOLDER))
}

pub fn load(dir: &Path) -> Option<(Live, Meta)> {
    let file = dir.join(INDEX_FILE);
    let loaded = index::decode(frame::read(&file, frame::INDEX)?);
    if loaded.is_none() {
        let _ = std::fs::remove_file(&file);
    }
    loaded.map(|(base, meta)| (Live::over(base), meta))
}

pub fn save(dir: &Path, live: &Live, meta: &Meta) -> io::Result<()> {
    if !live.settled() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Only a settled index can be saved"));
    }
    frame::write(&dir.join(INDEX_FILE), &index::encode(live.base(), meta)?)
}
