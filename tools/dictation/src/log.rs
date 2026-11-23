//! A short record of what dictation did, in `dictation/dictation.log` in the data folder, so a
//! problem can be traced on the computer it happened on. It never holds the words or the key.

use std::fmt::Display;
use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

/// Past this size the log starts again, keeping the last one as `dictation.old.log`.
const MOST: u64 = 256 * 1024;

static FILE: Mutex<Option<PathBuf>> = Mutex::new(None);

pub fn open(dir: &Path) {
    *FILE.lock().unwrap_or_else(PoisonError::into_inner) = Some(dir.join("dictation").join("dictation.log"));
}

pub fn note(line: impl Display) {
    let Some(file) = FILE.lock().unwrap_or_else(PoisonError::into_inner).clone() else { return };
    let file = file.as_path();
    if fs::metadata(file).is_ok_and(|meta| meta.len() > MOST) {
        let _ = fs::rename(file, file.with_file_name("dictation.old.log"));
    }
    if let Some(dir) = file.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let since = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    if let Ok(mut log) = OpenOptions::new().create(true).append(true).open(file) {
        let _ = writeln!(log, "{}.{:03} {line}", since.as_secs(), since.subsec_millis());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_go_to_the_data_folder() {
        let dir = std::env::temp_dir().join(format!("notchbase-dictation-log-{}", std::process::id()));
        let file = dir.join("dictation").join("dictation.log");
        // Engines started by other tests point the log elsewhere now and then, so try a few times.
        let written = (0..20).any(|_| {
            open(&dir);
            note(format_args!("key {:?}", "Down"));
            fs::read_to_string(&file).is_ok_and(|log| log.contains("key \"Down\""))
        });
        assert!(written);
        let _ = fs::remove_dir_all(dir);
    }
}
