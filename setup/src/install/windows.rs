//! The Windows download is the app itself, run without a console window.

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use notchbase_release::install::{self, CREATE_NO_WINDOW};

pub const DOWNLOAD: &str = "notchbase.exe";
pub const OS: &str = "Windows";
pub const SHORTCUTS: bool = true;

pub fn unpack(download: &Path, _dir: &Path) -> Result<PathBuf, String> {
    Ok(download.to_owned())
}

pub fn command(app: &Path) -> Command {
    let mut command = Command::new(app);
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

pub fn destination() -> Option<PathBuf> {
    install::windows_destination()
}

pub fn installed_binary() -> Option<PathBuf> {
    destination()
}

pub fn installed_version() -> Option<String> {
    install::registered_version()
}
