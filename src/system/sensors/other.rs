//! Other systems report no camera or microphone use.

use notchbase_sdk::protocol::Sensors;

pub(super) struct Probe;

impl Probe {
    pub(super) fn open() -> Self {
        Self
    }

    pub(super) fn read(&mut self) -> Sensors {
        Sensors::default()
    }

    pub(super) fn set_muted(&mut self, _muted: bool) {}
}
