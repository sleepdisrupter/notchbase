//! Small helpers for file names and the messages built from them.

use std::path::Path;

pub(super) fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |name| name.to_string_lossy().into_owned())
}

pub(super) fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .filter(|stem| !stem.is_empty())
        .unwrap_or_else(|| "file".to_owned())
}

pub(super) fn lowercase_extension(path: &Path) -> String {
    path.extension().map(|ext| ext.to_string_lossy().to_ascii_lowercase()).unwrap_or_default()
}

pub(super) fn couldnt_read(path: &Path) -> String {
    format!("Couldn't read {}", file_name(path))
}

pub(super) fn too_large(path: &Path) -> String {
    format!("{} is too large", file_name(path))
}

pub(super) fn counted(count: u64, noun: &str) -> String {
    match count {
        1 => format!("1 {noun}"),
        count => format!("{count} {noun}s"),
    }
}
