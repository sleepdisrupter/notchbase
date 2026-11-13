//! A key for spotting the same file twice: its canonical path, lower case where file names ignore case.

use std::fs;
use std::path::Path;

const FILE_NAMES_IGNORE_CASE: bool = cfg!(any(windows, target_os = "macos"));

pub(super) fn of(path: &Path) -> String {
    let resolved = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let key = without_verbatim_prefix(resolved.to_string_lossy().into_owned());
    if FILE_NAMES_IGNORE_CASE { key.to_lowercase() } else { key }
}

#[cfg(windows)]
fn without_verbatim_prefix(path: String) -> String {
    let path = if let Some(share) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{share}")
    } else {
        path.strip_prefix(r"\\?\").map(str::to_owned).unwrap_or(path)
    };
    path.replace('/', "\\")
}

#[cfg(not(windows))]
fn without_verbatim_prefix(path: String) -> String {
    path
}
