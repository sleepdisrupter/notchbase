//! Other systems have no install location, so installing isn't offered.

use std::io;
use std::path::Path;

use super::location::Target;

pub(super) const INSTALL_HINT: &str = "";

pub(super) fn destination(_current: &Target) -> io::Result<Target> {
    Err(io::Error::other("Installing is only for Windows and macOS"))
}

pub(super) fn copy_app(from: &Path, to: &Path) -> io::Result<()> {
    super::location::copy_tree(from, to)
}

pub(super) fn register(_home: &Target, _uninstall: &str) -> io::Result<()> {
    Ok(())
}

pub(super) fn unregister() {}

pub(super) fn note_version() {}

pub(super) fn desktop_shortcut(_home: &Target, _on: bool) -> io::Result<()> {
    Ok(())
}

pub(super) fn opens_at_login() -> bool {
    false
}

pub(super) fn set_login_item(_target: &Target, _on: bool) -> io::Result<()> {
    Err(io::Error::other("Opening at login is only for Windows and macOS"))
}

pub(super) fn remove_after_exit(_path: &Path) -> io::Result<()> {
    Ok(())
}
