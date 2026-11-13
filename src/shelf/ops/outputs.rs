//! Where results go: beside the source, or in the app's Shelf folder when that can't be written. Long encodes are
//! staged under a hidden name and moved into place when done.

use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::unique::{claim_free_name, move_into_free_name, numbered, write_new_file};

static NEXT_STAGE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug)]
pub(in crate::shelf) struct Outputs {
    fallback_dir: Option<PathBuf>,
}

impl Outputs {
    pub(in crate::shelf) fn new(fallback_dir: Option<PathBuf>) -> Self {
        Self { fallback_dir }
    }

    pub(super) fn write(&self, near: &Path, stem: &str, ext: &str, bytes: &[u8]) -> Result<PathBuf, String> {
        self.beside_or_in_fallback(near, |dir| write_new_file(dir, stem, ext, bytes))
            .map_err(|_| format!("Couldn't save {stem}.{ext}"))
    }

    pub(super) fn create_dir(&self, near: &Path, name: &str) -> Result<PathBuf, String> {
        self.beside_or_in_fallback(near, |dir| claim_free_name(dir, |n| numbered(name, n), |path| fs::create_dir(path)))
            .map(|(path, ())| path)
            .map_err(|_| format!("Couldn't create {name}"))
    }

    pub(super) fn stage(&self, near: &Path, stem: &str, ext: &str) -> Result<Staged, String> {
        self.beside_or_in_fallback(near, |dir| Staged::claim(dir, stem, ext))
            .map_err(|_| format!("Couldn't save {stem}.{ext}"))
    }

    fn beside_or_in_fallback<T>(&self, near: &Path, make: impl Fn(&Path) -> io::Result<T>) -> io::Result<T> {
        let beside = match near.parent() {
            Some(dir) if !dir.as_os_str().is_empty() => make(dir),
            _ => Err(io::ErrorKind::NotFound.into()),
        };
        match (beside, &self.fallback_dir) {
            (Err(_), Some(fallback_dir)) => {
                fs::create_dir_all(fallback_dir)?;
                make(fallback_dir)
            }
            (result, _) => result,
        }
    }
}

pub(super) struct Staged {
    dir: PathBuf,
    temp: PathBuf,
    stem: String,
    ext: String,
    committed: bool,
}

impl Staged {
    fn claim(dir: &Path, stem: &str, ext: &str) -> io::Result<Self> {
        let n = NEXT_STAGE.fetch_add(1, Ordering::Relaxed);
        let temp = dir.join(format!(".notchbase-{}-{n}.{ext}", std::process::id()));
        // Proves the folder can be written before a long encode starts. The encoder creates the file again itself.
        OpenOptions::new().write(true).create_new(true).open(&temp)?;
        fs::remove_file(&temp)?;
        Ok(Self { dir: dir.to_path_buf(), temp, stem: stem.to_owned(), ext: ext.to_owned(), committed: false })
    }

    pub(super) fn path(&self) -> &Path {
        &self.temp
    }

    pub(super) fn commit(mut self) -> Result<PathBuf, String> {
        let saved = move_into_free_name(&self.temp, &self.dir, &self.stem, &self.ext)
            .map_err(|_| format!("Couldn't save {}.{}", self.stem, self.ext))?;
        self.committed = true;
        Ok(saved)
    }
}

impl Drop for Staged {
    fn drop(&mut self) {
        if !self.committed {
            let _ = fs::remove_file(&self.temp);
        }
    }
}
