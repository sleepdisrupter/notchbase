//! Reads and writes the assistant's JSON config with care. Odd files are refused, the first edit keeps a backup,
//! and each save goes through a temporary file that's renamed into place.

use std::fs::{self, File};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use serde_json::{Map, Value};

const MAX_BYTES: u64 = 16 * 1024 * 1024;
const BACKUP_SUFFIX: &str = ".notchbase.bak";
const TEMP_SUFFIX: &str = ".notchbase.tmp";
const BYTE_ORDER_MARK: char = '\u{feff}';
const RENAME_ATTEMPTS: u32 = 5;
const RENAME_PAUSE: Duration = Duration::from_millis(40);

pub(super) struct Doc {
    pub(super) root: Map<String, Value>,
    pub(super) existed: bool,
    /// Whether the file ended in a newline, so a save keeps it.
    newline: bool,
}

pub(super) fn load(file: &Path) -> Result<Doc, String> {
    let size = match fs::metadata(file) {
        Ok(meta) => meta.len(),
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Ok(Doc { root: Map::new(), existed: false, newline: true });
        }
        Err(error) => return Err(format!("Couldn't read {}: {error}", file.display())),
    };
    if size > MAX_BYTES {
        return Err(format!("{} is too large to edit safely", file.display()));
    }
    let text = fs::read_to_string(file).map_err(|error| format!("Couldn't read {}: {error}", file.display()))?;
    let body = text.trim_start_matches(BYTE_ORDER_MARK);
    let newline = text.ends_with('\n');
    if body.trim().is_empty() {
        return Ok(Doc { root: Map::new(), existed: true, newline });
    }
    match serde_json::from_str(body) {
        Ok(Value::Object(root)) => Ok(Doc { root, existed: true, newline }),
        Ok(_) => Err(format!("{} isn't a JSON object, so it was left untouched", file.display())),
        Err(error) => Err(format!("{} isn't valid JSON ({error}), so it was left untouched", file.display())),
    }
}

pub(super) fn save(file: &Path, root: &Map<String, Value>, doc: &Doc) -> Result<(), String> {
    let target = resolve(file);
    let mut text = serde_json::to_string_pretty(root).map_err(|error| error.to_string())?;
    if doc.newline {
        text.push('\n');
    }
    if doc.existed {
        back_up(&target)?;
    }
    write_atomically(&target, text.as_bytes()).map_err(|error| format!("Couldn't write {}: {error}", target.display()))
}

/// Follows a symlink, so the real file is replaced rather than the link.
fn resolve(file: &Path) -> PathBuf {
    let linked = fs::symlink_metadata(file).is_ok_and(|meta| meta.file_type().is_symlink());
    if linked { fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf()) } else { file.to_path_buf() }
}

fn sibling(file: &Path, suffix: &str) -> PathBuf {
    let mut name = file.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    file.with_file_name(name)
}

fn back_up(file: &Path) -> Result<(), String> {
    let backup = sibling(file, BACKUP_SUFFIX);
    if backup.exists() {
        return Ok(());
    }
    fs::copy(file, &backup).map(|_| ()).map_err(|error| format!("Couldn't back up {}: {error}", file.display()))
}

fn write_atomically(file: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let temp = sibling(file, TEMP_SUFFIX);
    let written = write_temp(file, &temp, bytes).and_then(|()| replace(&temp, file));
    if written.is_err() {
        let _ = fs::remove_file(&temp);
    }
    written
}

fn write_temp(file: &Path, temp: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut out = File::create(temp)?;
    out.write_all(bytes)?;
    out.sync_all()?;
    drop(out);
    keep_permissions(file, temp)
}

#[cfg(unix)]
fn keep_permissions(file: &Path, temp: &Path) -> std::io::Result<()> {
    match fs::metadata(file) {
        Ok(meta) => fs::set_permissions(temp, meta.permissions()),
        Err(_) => Ok(()),
    }
}

#[cfg(not(unix))]
fn keep_permissions(_file: &Path, _temp: &Path) -> std::io::Result<()> {
    Ok(())
}

/// Retries the rename briefly, in case the file is locked for a moment.
fn replace(temp: &Path, file: &Path) -> std::io::Result<()> {
    let mut attempt = 1;
    loop {
        match fs::rename(temp, file) {
            Err(_) if attempt < RENAME_ATTEMPTS => {
                attempt += 1;
                thread::sleep(RENAME_PAUSE);
            }
            outcome => return outcome,
        }
    }
}
