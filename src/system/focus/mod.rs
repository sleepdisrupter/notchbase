//! Focus and Do Not Disturb, read every two seconds, so the notch can show which mode is on.

#[cfg(any(target_os = "macos", test))]
mod parse;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use self::macos as imp;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
use self::windows as imp;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod other;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use self::other as imp;

use std::time::Duration;

use notchbase_sdk::protocol::Focus;

use crate::poll::Poll;

const EVERY: Duration = Duration::from_secs(2);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reading {
    // Only the Mac reads a file that can be locked away from us.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    Unreadable,
    Seen(Focus),
}

pub struct FocusWatch {
    poll: Poll<Reading>,
}

impl FocusWatch {
    pub fn start() -> Self {
        Self { poll: Poll::start("notchbase-focus", EVERY, || imp::read) }
    }

    pub fn take(&self) -> Option<Reading> {
        self.poll.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "reads this computer's real Focus, which on a Mac needs Full Disk Access"]
    fn this_computers_focus_can_be_read() {
        eprintln!("{:?}", imp::read());
    }
}
