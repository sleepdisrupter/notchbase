//! Whether the screen is locked, read five times a second.

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

use crate::poll::Poll;

const EVERY: Duration = Duration::from_millis(200);

pub struct ScreenLock {
    poll: Poll<bool>,
}

impl ScreenLock {
    pub fn start() -> Self {
        Self { poll: Poll::start("notchbase-session", EVERY, || imp::locked) }
    }

    pub fn take(&self) -> Option<bool> {
        self.poll.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "asks this computer whether its screen is locked"]
    fn session_in_use_is_unlocked() {
        assert!(!imp::locked());
    }
}
