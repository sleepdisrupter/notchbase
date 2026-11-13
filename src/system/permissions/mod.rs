//! Privacy permissions: what each feature needs, whether it was granted, and asking for it. A `Gate` holds a watcher
//! back until its access is granted or it may ask.

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "macos")]
use self::macos as imp;

#[cfg(not(target_os = "macos"))]
mod other;
#[cfg(not(target_os = "macos"))]
use self::other as imp;
mod probe;

pub use probe::{Prober, is_command as is_probe, run as run_probe};

use crate::config::{self, Walkthrough};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Access {
    Accessibility,
    Bluetooth,
    Calendar,
    Downloads,
    Automation,
    SystemAudio,
    ScreenRecording,
    FullDisk,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Standing {
    Granted,
    Denied,
    Undecided,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Asking {
    Prompt,
    Settings,
}

impl Access {
    pub const ALL: [Access; 8] = [
        Access::Accessibility,
        Access::Bluetooth,
        Access::Calendar,
        Access::Downloads,
        Access::Automation,
        Access::SystemAudio,
        Access::ScreenRecording,
        Access::FullDisk,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Access::Accessibility => "accessibility",
            Access::Bluetooth => "bluetooth",
            Access::Calendar => "calendar",
            Access::Downloads => "downloads",
            Access::Automation => "automation",
            Access::SystemAudio => "system_audio",
            Access::ScreenRecording => "screen_recording",
            Access::FullDisk => "full_disk",
        }
    }

    pub fn asking(self) -> Asking {
        match self {
            Access::Accessibility | Access::ScreenRecording | Access::FullDisk => Asking::Settings,
            _ => Asking::Prompt,
        }
    }
}

pub const SUPPORTED: bool = imp::SUPPORTED;

pub fn standing(access: Access) -> Option<Standing> {
    imp::standing(access)
}

pub fn ask(access: Access) {
    imp::ask(access);
}

pub fn may_ask(access: Access) -> bool {
    let config = config::current();
    asks_in_the_background(&config.permissions, access)
}

fn asks_in_the_background(permissions: &config::Permissions, access: Access) -> bool {
    permissions.walkthrough == Walkthrough::Done && !permissions.was_asked(access.key())
}

pub struct Gate {
    access: Access,
    open: bool,
}

impl Gate {
    pub fn new(access: Access) -> Self {
        Self { access, open: !SUPPORTED }
    }

    pub fn open(&mut self) -> bool {
        if !self.open {
            self.open = standing(self.access) == Some(Standing::Granted) || may_ask(self.access);
        }
        self.open
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Permissions;

    fn permissions(walkthrough: Walkthrough, asked: &[Access]) -> Permissions {
        Permissions { walkthrough, asked: asked.iter().map(|a| a.key().to_owned()).collect(), ..Permissions::default() }
    }

    #[test]
    fn no_asking_before_walkthrough() {
        let due = permissions(Walkthrough::Pending, &[]);
        assert!(Access::ALL.iter().all(|&access| !asks_in_the_background(&due, access)));
    }

    #[test]
    fn after_it_only_unasked_may_ask() {
        let done = permissions(Walkthrough::Done, &[Access::Bluetooth, Access::Accessibility]);
        assert!(!asks_in_the_background(&done, Access::Bluetooth));
        assert!(!asks_in_the_background(&done, Access::Accessibility));
        assert!(asks_in_the_background(&done, Access::Automation), "a player that wasn't open is asked when it is");
        let before = permissions(Walkthrough::Done, &[]);
        assert!(Access::ALL.iter().all(|&access| asks_in_the_background(&before, access)), "as it always did");
    }

    #[test]
    fn every_permission_has_its_own_key() {
        let keys: std::collections::HashSet<&str> = Access::ALL.iter().map(|access| access.key()).collect();
        assert_eq!(keys.len(), Access::ALL.len());
    }

    #[test]
    fn settings_only_ones_open_settings() {
        let settings: Vec<Access> =
            Access::ALL.into_iter().filter(|access| access.asking() == Asking::Settings).collect();
        assert_eq!(settings, [Access::Accessibility, Access::ScreenRecording, Access::FullDisk]);
    }
}
