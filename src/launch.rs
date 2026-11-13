//! What this run of the executable is for, read from its first argument: an assistant hook, an install, one of
//! the commands, or the app itself. Only the app claims the single-instance lock.

use std::ffi::OsString;

use crate::system::permissions;
use crate::{assistant, installer, platform};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Launch {
    Hook,
    Installer,
    /// A fresh copy printing the permission answers, since macOS keeps some for the life of a process.
    Probe,
    /// No arguments and a pipe for input: a hook runner that dropped the hook's arguments. It exits at once.
    Stray,
    App,
}

impl Launch {
    pub fn current() -> Self {
        Self::of(&std::env::args_os().skip(1).collect::<Vec<_>>(), platform::input_is_piped())
    }

    fn of(args: &[OsString], piped: bool) -> Self {
        let first = args.first().map(OsString::as_os_str);
        if assistant::is_hook_launch(first) {
            Self::Hook
        } else if installer::is_command(first) {
            Self::Installer
        } else if permissions::is_probe(first) {
            Self::Probe
        } else if first.is_none() && piped {
            Self::Stray
        } else {
            Self::App
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn launch(args: &[&str]) -> Launch {
        launch_with(args, false)
    }

    fn launch_with(args: &[&str], piped: bool) -> Launch {
        Launch::of(&args.iter().map(OsString::from).collect::<Vec<_>>(), piped)
    }

    #[test]
    fn hooks_never_touch_the_instance_lock() {
        assert_eq!(launch(&["hook", "claude"]), Launch::Hook);
        assert_eq!(launch(&["hook", "codex", "agent-turn-complete"]), Launch::Hook);
    }

    #[test]
    fn installer_runs_skip_the_lock() {
        assert_eq!(launch(&["--install", "--login"]), Launch::Installer);
        assert_eq!(launch(&["--uninstall"]), Launch::Installer);
    }

    #[test]
    fn permission_helper_skips_the_lock() {
        assert_eq!(launch(&["--permission-standings"]), Launch::Probe);
        assert_eq!(launch_with(&["--permission-standings"], true), Launch::Probe);
    }

    #[test]
    fn app_itself_claims_the_lock() {
        assert_eq!(launch(&[]), Launch::App);
        assert_eq!(launch(&["publish"]), Launch::App);
        assert_eq!(launch(&["tool.wasm", "--publish"]), Launch::App);
        assert_eq!(launch(&["--replaces", "42"]), Launch::App);
        assert_eq!(launch(&["--installed", "7"]), Launch::App);
        assert_eq!(launch(&["hooks"]), Launch::App);
    }

    #[test]
    fn piped_bare_launch_is_a_lost_hook() {
        assert_eq!(launch_with(&[], true), Launch::Stray);
        assert_eq!(launch_with(&["hook", "claude"], true), Launch::Hook);
        assert_eq!(launch_with(&["--uninstall"], true), Launch::Installer);
        assert_eq!(launch_with(&["--replaces", "42"], true), Launch::App);
    }
}
