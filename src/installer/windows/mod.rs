//! Installing on Windows, for this user only: the app goes to `%LOCALAPPDATA%\Programs\notchbase`, with a Start
//! menu shortcut, an entry in the Apps list and a Run value for opening at login.

mod registry;
mod shortcut;

use std::io;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use notchbase_release::install::{CREATE_NO_WINDOW, DISPLAY_VERSION, UNINSTALL_KEY};

use super::location::{Target, copy_tree};
use registry::Data;

pub(super) const INSTALL_HINT: &str = "Add it to Start and open it at login";

const NAME: &str = "notchbase";
const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const START_MENU: &str = r"Microsoft\Windows\Start Menu\Programs";
const KIB: u64 = 1024;

pub(super) fn destination(_current: &Target) -> io::Result<Target> {
    let exe = notchbase_release::install::windows_destination();
    exe.map(Target::Binary).ok_or_else(|| io::Error::other("Couldn't find your app folder"))
}

pub(super) fn copy_app(from: &Path, to: &Path) -> io::Result<()> {
    copy_tree(from, to)
}

/// Adds the Start menu shortcut and the Apps list entry, with the command that uninstalls.
pub(super) fn register(home: &Target, uninstall: &str) -> io::Result<()> {
    let exe = home.path();
    let link = start_menu()?;
    if let Some(dir) = link.parent() {
        std::fs::create_dir_all(dir)?;
    }
    shortcut::create(&link, exe)?;
    let shown = exe.display().to_string();
    let folder = exe.parent().map(|dir| dir.display().to_string()).unwrap_or_default();
    let size = std::fs::metadata(exe).map_or(0, |meta| meta.len().div_ceil(KIB)) as u32;
    let values = [
        ("DisplayName", Data::Text(NAME)),
        (DISPLAY_VERSION, Data::Text(env!("CARGO_PKG_VERSION"))),
        ("Publisher", Data::Text(NAME)),
        ("DisplayIcon", Data::Text(&shown)),
        ("InstallLocation", Data::Text(&folder)),
        ("UninstallString", Data::Text(uninstall)),
        ("EstimatedSize", Data::Number(size)),
        ("NoModify", Data::Number(1)),
        ("NoRepair", Data::Number(1)),
    ];
    for (name, data) in values {
        registry::set(UNINSTALL_KEY, name, data)?;
    }
    Ok(())
}

/// Keeps the version in the Apps list up to date after an update.
pub(super) fn note_version() {
    let listed = registry::get(UNINSTALL_KEY, DISPLAY_VERSION);
    if listed.is_some_and(|version| version != env!("CARGO_PKG_VERSION")) {
        let _ = registry::set(UNINSTALL_KEY, DISPLAY_VERSION, Data::Text(env!("CARGO_PKG_VERSION")));
    }
}

pub(super) fn unregister() {
    for link in [start_menu(), desktop()].into_iter().flatten() {
        let _ = std::fs::remove_file(link);
    }
    let _ = registry::delete_key(UNINSTALL_KEY);
}

pub(super) fn opens_at_login() -> bool {
    registry::get(RUN, NAME).is_some()
}

pub(super) fn set_login_item(target: &Target, on: bool) -> io::Result<()> {
    if on {
        registry::set(RUN, NAME, Data::Text(&format!("\"{}\"", target.path().display())))
    } else {
        registry::delete_value(RUN, NAME)
    }
}

/// Windows won't delete a running program's folder, so cmd does it once this process has gone.
pub(super) fn remove_after_exit(path: &Path) -> io::Result<()> {
    Command::new("cmd")
        // cmd has its own quoting rules, so the script goes over exactly as written.
        .raw_arg(removal_script(path))
        .current_dir(std::env::temp_dir())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map(drop)
}

/// `ping -n 3` waits about two seconds, long enough for this process to exit, before `rmdir` runs.
fn removal_script(path: &Path) -> String {
    format!("/s /c \"ping -n 3 127.0.0.1 >nul & rmdir /s /q \"{}\"\"", path.display())
}

pub(super) fn desktop_shortcut(home: &Target, on: bool) -> io::Result<()> {
    let link = desktop()?;
    match on {
        true => shortcut::create(&link, home.path()),
        false => Ok(()),
    }
}

fn desktop() -> io::Result<PathBuf> {
    let folder = crate::platform::known_dir(crate::platform::KnownDir::Desktop)
        .ok_or_else(|| io::Error::other("Couldn't find the desktop"))?;
    Ok(folder.join(format!("{NAME}.lnk")))
}

fn start_menu() -> io::Result<PathBuf> {
    let roaming = std::env::var_os("APPDATA").ok_or_else(|| io::Error::other("Couldn't find the Start menu"))?;
    Ok(PathBuf::from(roaming).join(START_MENU).join(format!("{NAME}.lnk")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmd_removes_leftover_after_a_wait() {
        let script = removal_script(Path::new(r"C:\Users\some one\AppData\Local\Programs\notchbase"));
        let expected =
            r#"/s /c "ping -n 3 127.0.0.1 >nul & rmdir /s /q "C:\Users\some one\AppData\Local\Programs\notchbase"""#;
        assert_eq!(script, expected);
    }
}
