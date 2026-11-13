//! Other systems have nothing to install.

use std::path::{Path, PathBuf};
use std::process::Command;

pub const DOWNLOAD: &str = "notchbase";
pub const OS: &str = "this computer";
pub const SHORTCUTS: bool = false;

pub fn unpack(_download: &Path, _dir: &Path) -> Result<PathBuf, String> {
    Err("notchbase installs on Windows and macOS".into())
}

pub fn command(app: &Path) -> Command {
    Command::new(app)
}

pub fn destination() -> Option<PathBuf> {
    None
}

pub fn installed_binary() -> Option<PathBuf> {
    None
}

pub fn installed_version() -> Option<String> {
    None
}
