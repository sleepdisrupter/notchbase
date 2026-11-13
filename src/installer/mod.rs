//! `--install` and `--uninstall`: copying the app to its home, registering it, keeping settings across an
//! uninstall, and handing over to the copy that was just put in place.

mod handoff;
mod location;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(any(windows, target_os = "macos")))]
mod other;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "macos")]
use macos as imp;
#[cfg(not(any(windows, target_os = "macos")))]
use other as imp;
#[cfg(windows)]
use windows as imp;

use std::io;
use std::path::PathBuf;

use notchbase_release::install::{ERROR_PREFIX, INSTALL, UNINSTALL};

use location::{keep_settings, restore_settings};

pub use handoff::{Arrival, arrive, relaunch};
pub use location::{Target, aside, carry_settings, remove, swap};
pub use notchbase_release::install::Choices;

pub const INSTALL_HINT: &str = imp::INSTALL_HINT;

const KEPT: &str = "kept-settings";

/// Whether the first argument is `--install` or `--uninstall`.
pub fn is_command(first: Option<&std::ffi::OsStr>) -> bool {
    first.is_some_and(|first| first == INSTALL || first == UNINSTALL)
}

/// Runs the install or uninstall the arguments ask for, and gives the exit code. None for any other launch.
pub fn run_from_args() -> Option<i32> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let done = match args.first()?.as_str() {
        INSTALL => install(Choices::from_args(&args)),
        UNINSTALL => uninstall(),
        _ => return None,
    };
    Some(match done {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("{ERROR_PREFIX}{err}");
            1
        }
    })
}

/// Whether this copy has a home to install to. A Mac installs only from a `.app`.
pub fn can_install() -> bool {
    Target::current().is_ok_and(|current| imp::destination(&current).is_ok())
}

/// Whether this copy is the one in its home.
pub fn is_installed() -> bool {
    Target::current().is_ok_and(|current| imp::destination(&current).is_ok_and(|home| current.same_place(&home)))
}

/// Stops other copies, puts this one in its home with its settings, registers it and relaunches from there.
pub fn install(choices: Choices) -> io::Result<()> {
    let current = Target::current()?;
    let home = imp::destination(&current)?;
    handoff::stop_others();
    if !current.same_place(&home) {
        if let Some(folder) = home.path().parent() {
            std::fs::create_dir_all(folder)?;
        }
        swap(home.path(), |to| imp::copy_app(current.path(), to))?;
        carry_settings(&current, &home);
        if let Some(kept) = kept() {
            restore_settings(&kept, &home);
        }
    }
    if choices.changes_settings() {
        settle(&home, choices)?;
    }
    imp::register(&home, &format!("\"{}\" {UNINSTALL}", home.path().display()))?;
    imp::set_login_item(&home, choices.login)?;
    imp::desktop_shortcut(&home, choices.desktop)?;
    relaunch(&home, Arrival::Installed)
}

fn settle(home: &Target, choices: Choices) -> io::Result<()> {
    let folder = home.exe_dir().ok_or_else(|| io::Error::other("The app has no folder"))?;
    let file = crate::platform::config_file_in(&folder);
    let (mut store, _) = crate::config::ConfigStore::open(file, None);
    store.update(|config| {
        if choices.updates_off {
            config.updates.automatic = false;
        }
        if choices.assistant_off {
            config.assistant.enabled = false;
        }
    })
}

/// Unregisters the app and keeps its settings, then deletes it after this process exits.
pub fn uninstall() -> io::Result<()> {
    let current = Target::current()?;
    handoff::stop_others();
    let home = imp::destination(&current);
    let installed = home.as_ref().is_ok_and(|home| current.same_place(home));
    imp::set_login_item(&current, false)?;
    imp::unregister();
    let home = home?;
    if !installed {
        return Ok(());
    }
    let leftover = match &home {
        Target::Binary(exe) => exe.parent().unwrap_or(exe),
        Target::Bundle(app) => app,
    };
    let kept = kept().ok_or_else(|| io::Error::other("Couldn't find where to keep your settings"))?;
    keep_settings(&home, &kept)?;
    imp::remove_after_exit(leftover)
}

fn kept() -> Option<PathBuf> {
    crate::platform::data_dir().map(|dir| dir.join(KEPT))
}

/// Keeps the version the system lists up to date after an update.
pub fn note_version() {
    if is_installed() {
        imp::note_version();
    }
}

pub fn opens_at_login() -> bool {
    imp::opens_at_login()
}

pub fn set_open_at_login(on: bool) -> io::Result<()> {
    imp::set_login_item(&Target::current()?, on)
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::*;

    #[test]
    fn only_install_flags_are_commands() {
        assert!(is_command(Some(OsStr::new("--install"))));
        assert!(is_command(Some(OsStr::new("--uninstall"))));
        assert!(!is_command(Some(OsStr::new("--installed"))));
        assert!(!is_command(Some(OsStr::new("install"))));
        assert!(!is_command(Some(OsStr::new(""))));
        assert!(!is_command(None));
    }
}
