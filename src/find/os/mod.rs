//! What Find needs from the system, behind the same calls on every platform: the roots to index, the folders that
//! are sealed or guarded, listing a folder, statting a path, and watching for changes.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as imp;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
use windows as imp;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod other;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use other as imp;

#[cfg(any(test, not(target_os = "macos")))]
mod portable;

use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

use super::shared::Order;
use super::watch::Watcher;

pub const KEEPS_CURSOR: bool = imp::KEEPS_CURSOR;

pub fn roots(volumes: bool) -> Vec<Root> {
    imp::roots(volumes)
}

pub fn sealed() -> Vec<PathBuf> {
    imp::sealed()
}

pub fn unsealed() -> Vec<PathBuf> {
    imp::unsealed()
}

pub fn guarded(home: &Path) -> Vec<PathBuf> {
    imp::guarded(home)
}

pub fn launchers(home: Option<&Path>) -> Vec<PathBuf> {
    imp::launchers(home)
}

pub fn full_access() -> bool {
    imp::full_access()
}

pub fn list(dir: &Path, devices: [u64; 2], into: &mut Listed) -> io::Result<()> {
    imp::list(dir, devices, into)
}

pub fn stat(path: &Path) -> io::Result<Option<Raw>> {
    imp::stat(path)
}

pub fn be_gentle() {
    imp::be_gentle();
}

pub fn cursor_now() -> u64 {
    imp::cursor_now()
}

pub fn watch(roots: &[Root], since: Option<u64>, sink: Sender<Order>) -> Option<Watcher> {
    imp::watch(roots, since, sink)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Root {
    pub path: PathBuf,
    pub volume: [u8; 16],
    pub devices: [u64; 2],
    pub flags: u8,
}

pub mod root {
    pub const BOOT: u8 = 1;
    pub const REMOVABLE: u8 = 2;
}

pub mod raw {
    pub const DIR: u8 = 1;
    pub const HIDDEN: u8 = 2;
    pub const CLOUD: u8 = 4;
    pub const LINK: u8 = 8;
    pub const DATALESS: u8 = 16;
    pub const ELSEWHERE: u8 = 32;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Raw {
    pub modified: u32,
    pub created: u32,
    pub file_id: u64,
    pub bits: u8,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Listed {
    pub names: Vec<u8>,
    pub rows: Vec<Raw>,
}
