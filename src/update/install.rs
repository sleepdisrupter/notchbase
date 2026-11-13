//! Putting a downloaded update in place: the running copy is swapped for it, and the new one is started.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use notchbase_release::install;

use crate::installer::{Arrival, Target, aside, carry_settings, relaunch, remove, swap};

const DITTO: &str = "/usr/bin/ditto";

/// Unpacks a Mac update's zip and finds what to run in it. A Windows update is the executable itself.
pub fn prepare(download: &Path, into: &Path) -> io::Result<PathBuf> {
    if !cfg!(target_os = "macos") {
        return Ok(download.to_owned());
    }
    remove(into);
    std::fs::create_dir_all(into)?;
    ditto(&mut install::unzip(download, into))?;
    let app = install::app_in(into)?.ok_or_else(|| io::Error::other("The update had no app inside"))?;
    Ok(match Target::current()? {
        Target::Bundle(_) => app,
        Target::Binary(exe) => app.join("Contents").join("MacOS").join(exe.file_name().unwrap_or_default()),
    })
}

/// Swaps the running copy for the new one and starts it. A Mac bundle takes the old one's settings along.
pub fn install(new: &Path) -> io::Result<()> {
    let target = Target::current()?;
    match &target {
        Target::Binary(exe) => swap(exe, |to| std::fs::copy(new, to).map(drop))?,
        Target::Bundle(app) => {
            // A rename is quickest, and `ditto` copies instead when a rename can't, as across volumes.
            swap(app, |to| std::fs::rename(new, to).or_else(|_| ditto(Command::new(DITTO).arg(new).arg(to))))?;
            carry_settings(&Target::Bundle(aside(app)), &target);
        }
    }
    relaunch(&target, Arrival::Updated)
}

fn ditto(command: &mut Command) -> io::Result<()> {
    let status = command.status()?;
    if status.success() { Ok(()) } else { Err(io::Error::other("Couldn't unpack the update")) }
}
