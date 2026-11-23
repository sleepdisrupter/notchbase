//! Whether Mission Control or App Exposé is showing, read about seven times a second, so the notch stays closed
//! over it.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use self::macos as imp;

#[cfg(not(target_os = "macos"))]
mod other;
#[cfg(not(target_os = "macos"))]
use self::other as imp;

use std::time::Duration;

use crate::poll::Poll;

const EVERY: Duration = Duration::from_millis(150);

pub struct MissionControl {
    poll: Poll<bool>,
}

impl MissionControl {
    pub fn start() -> Self {
        Self { poll: Poll::start("notchbase-mission-control", EVERY, imp::reader) }
    }

    pub fn take(&self) -> Option<bool> {
        self.poll.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "asks this computer whether Mission Control is showing"]
    fn not_showing_while_testing() {
        assert!(!imp::reader()());
    }
}
