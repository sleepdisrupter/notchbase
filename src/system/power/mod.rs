//! The battery: its level, whether it's plugged in and whether it's charging, read every two seconds.

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

use notchbase_sdk::protocol::Power;

use crate::poll::Poll;

const EVERY: Duration = Duration::from_secs(2);

pub struct Battery {
    poll: Poll<Power>,
}

pub fn read() -> Power {
    imp::read()
}

impl Battery {
    pub fn start() -> Self {
        Self { poll: Poll::start("notchbase-power", EVERY, || imp::read) }
    }

    pub fn take(&self) -> Option<Power> {
        self.poll.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "reads this computer's real battery"]
    fn power_source_can_be_read() {
        let power = imp::read();
        eprintln!("{power:?}");
        assert!(power.level.is_none_or(|level| level <= 100));
        assert!(!power.charging || power.plugged, "a battery only charges while plugged in");
    }
}
