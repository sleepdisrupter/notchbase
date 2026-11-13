mod apps;
mod contract;
pub(in crate::find) mod corpus;
mod engine;
pub(crate) mod real;
mod timings;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

pub(in crate::find) struct Scratch(PathBuf);

impl Scratch {
    pub(in crate::find) fn new(name: &str) -> Self {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("notchbase-find-{name}-{}-{id}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a folder under the system temp directory");
        Self(dir)
    }

    pub(in crate::find) fn dir(&self) -> &Path {
        &self.0
    }

    pub(in crate::find) fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
