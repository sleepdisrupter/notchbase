//! Other systems have no volume control.

use super::{AppId, AudioApp, AudioDevice, DeviceKind};

pub struct Backend;

impl Backend {
    pub fn new() -> Self {
        Self
    }

    pub fn is_supported(&self) -> bool {
        false
    }

    pub fn unsupported_reason(&self) -> Option<&str> {
        Some("Volume control isn't available on this system")
    }

    pub fn refresh(&mut self) {}

    pub fn update_levels(&mut self) {}

    pub fn maintain(&mut self) {}

    pub fn apps(&self) -> &[AudioApp] {
        &[]
    }

    pub fn master(&self) -> Option<&AudioApp> {
        None
    }

    pub fn input(&self) -> Option<&AudioApp> {
        None
    }

    pub fn devices(&self, _kind: DeviceKind) -> &[AudioDevice] {
        &[]
    }

    pub fn set_volume(&mut self, _id: AppId, _volume: f32) {}

    pub fn set_muted(&mut self, _id: AppId, _muted: bool) {}

    pub fn set_default_device(&mut self, _kind: DeviceKind, _id: &str) {}
}
