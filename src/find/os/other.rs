//! Other systems: home is the only root, nothing is watched, and nothing is sealed or guarded.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

use crate::find::shared::Order;
use crate::find::watch::Watcher;
use crate::platform::{self, KnownDir};

use super::{Listed, Raw, Root, portable, root};

pub const KEEPS_CURSOR: bool = false;

pub fn roots(_volumes: bool) -> Vec<Root> {
    let home = platform::known_dir(KnownDir::Home);
    home.map(|path| Root { devices: [device(&path); 2], path, volume: [0; 16], flags: root::BOOT })
        .into_iter()
        .collect()
}

pub fn sealed() -> Vec<PathBuf> {
    Vec::new()
}

pub fn unsealed() -> Vec<PathBuf> {
    Vec::new()
}

pub fn guarded(_home: &Path) -> Vec<PathBuf> {
    Vec::new()
}

pub fn launchers(_home: Option<&Path>) -> Vec<PathBuf> {
    Vec::new()
}

pub fn full_access() -> bool {
    true
}

pub fn list(dir: &Path, devices: [u64; 2], into: &mut Listed) -> io::Result<()> {
    portable::list(dir, devices, into)
}

pub fn stat(path: &Path) -> io::Result<Option<Raw>> {
    portable::stat(path)
}

pub fn be_gentle() {}

pub fn cursor_now() -> u64 {
    0
}

pub fn watch(_roots: &[Root], _since: Option<u64>, _sink: Sender<Order>) -> Option<Watcher> {
    None
}

#[cfg(unix)]
fn device(path: &Path) -> u64 {
    use std::os::unix::fs::MetadataExt as _;

    std::fs::metadata(path).map_or(0, |found| found.dev())
}

#[cfg(not(unix))]
fn device(_path: &Path) -> u64 {
    0
}
