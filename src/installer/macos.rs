//! Installing on a Mac: the bundle goes to Applications, and a launch agent opens it at login.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use notchbase_release::install;

use super::location::Target;

pub(super) const INSTALL_HINT: &str = "Move it to Applications and open it at login";

const LABEL: &str = "app.notchbase";
const DITTO: &str = "/usr/bin/ditto";
const OPEN: &str = "/usr/bin/open";

pub(super) fn destination(current: &Target) -> io::Result<Target> {
    let Target::Bundle(app) = current else {
        return Err(io::Error::other("Install notchbase from notchbase.app"));
    };
    let name = app.file_name().ok_or_else(|| io::Error::other("The app has no name"))?;
    install::mac_destination(name).map(Target::Bundle).ok_or_else(no_home)
}

/// Copies the bundle with `ditto`, which keeps its links and extended attributes, so the signature stays valid.
pub(super) fn copy_app(from: &Path, to: &Path) -> io::Result<()> {
    let status = Command::new(DITTO).arg(from).arg(to).status()?;
    if status.success() { Ok(()) } else { Err(io::Error::other("Couldn't copy the app")) }
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
    agent().is_ok_and(|agent| agent.exists())
}

pub(super) fn set_login_item(target: &Target, on: bool) -> io::Result<()> {
    let agent = agent()?;
    if !on {
        return match std::fs::remove_file(&agent) {
            Err(err) if err.kind() != io::ErrorKind::NotFound => Err(err),
            _ => Ok(()),
        };
    }
    if let Some(dir) = agent.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(agent, plist(target))
}

/// Deletes the app from a shell two seconds later, once this process has gone.
pub(super) fn remove_after_exit(path: &Path) -> io::Result<()> {
    Command::new("/bin/sh").args(["-c", "sleep 2; rm -rf \"$1\"", "sh"]).arg(path).spawn().map(drop)
}

/// The launch agent. A bundle is opened with `open -a`, so it starts through Launch Services like any app.
fn plist(target: &Target) -> String {
    let path = target.path().to_string_lossy();
    let program: Vec<&str> = match target {
        Target::Bundle(_) => vec![OPEN, "-a", &path],
        Target::Binary(_) => vec![&path],
    };
    let arguments: String = program.iter().map(|arg| format!("\n        <string>{}</string>", escape(arg))).collect();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n\
         <dict>\n    \
         <key>Label</key>\n    <string>{LABEL}</string>\n    \
         <key>ProgramArguments</key>\n    <array>{arguments}\n    </array>\n    \
         <key>RunAtLoad</key>\n    <true/>\n\
         </dict>\n\
         </plist>\n"
    )
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn home() -> io::Result<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from).ok_or_else(no_home)
}

fn no_home() -> io::Error {
    io::Error::other("Couldn't find your home folder")
}

fn agent() -> io::Result<PathBuf> {
    Ok(home()?.join("Library").join("LaunchAgents").join(format!("{LABEL}.plist")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_item_opens_via_launch_services() {
        let text = plist(&Target::Bundle(PathBuf::from("/Applications/R&D notchbase.app")));
        assert!(text.contains("<string>/usr/bin/open</string>"));
        assert!(text.contains("<string>/Applications/R&amp;D notchbase.app</string>"));
        assert!(text.contains("<key>RunAtLoad</key>"));
    }
}
