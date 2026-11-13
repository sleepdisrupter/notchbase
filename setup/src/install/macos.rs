//! The Mac download is a zipped app bundle that goes into Applications.

use std::path::{Path, PathBuf};
use std::process::Command;

use notchbase_release::install::{self, BUNDLE, BUNDLE_BINARY};

pub const DOWNLOAD: &str = "notchbase.zip";
pub const OS: &str = "macOS";
pub const SHORTCUTS: bool = false;

const NO_APP: &str = "The download holds no app";

pub fn unpack(download: &Path, dir: &Path) -> Result<PathBuf, String> {
    let status = install::unzip(download, dir).status();
    if !status.is_ok_and(|status| status.success()) {
        return Err("Couldn't unpack notchbase.app".into());
    }
    let app = install::app_in(dir).map_err(|err| format!("Couldn't unpack notchbase.app: {err}"))?.ok_or(NO_APP)?;
    Ok(app.join(BUNDLE_BINARY))
}

pub fn command(app: &Path) -> Command {
    Command::new(app)
}

pub fn destination() -> Option<PathBuf> {
    install::mac_destination(BUNDLE.as_ref())
}

pub fn installed_binary() -> Option<PathBuf> {
    destination().map(|app| app.join(BUNDLE_BINARY))
}

pub fn installed_version() -> Option<String> {
    install::bundle_version(&destination()?)
}
