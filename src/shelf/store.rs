//! Saves the shelf's paths to shelf.json, writing a temp file first and renaming it into place.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Default)]
struct SavedShelf {
    paths: Vec<String>,
}

pub(super) fn load(file: &Path) -> Vec<PathBuf> {
    let Ok(text) = fs::read_to_string(file) else { return Vec::new() };
    let saved: SavedShelf = serde_json::from_str(&text).unwrap_or_default();
    saved.paths.into_iter().map(PathBuf::from).collect()
}

pub(super) fn save<'a>(file: &Path, paths: impl Iterator<Item = &'a Path>) -> io::Result<()> {
    let saved = SavedShelf { paths: paths.filter_map(|path| path.to_str().map(str::to_owned)).collect() };
    let json = serde_json::to_string_pretty(&saved)?;
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir)?;
    }
    let unfinished = file.with_extension("json.tmp");
    fs::write(&unfinished, json)?;
    fs::rename(&unfinished, file)
}
