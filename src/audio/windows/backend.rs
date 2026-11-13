//! The Windows backend: the default endpoints and the app sessions on the default output.

use super::com::Apartment;
use super::endpoints::Endpoints;
use super::sessions::Sessions;
use crate::audio::{AppId, AudioApp, AudioDevice, DeviceKind};

const COM_UNAVAILABLE: &str = "Windows audio services aren't available";

pub struct Backend {
    endpoints: Endpoints,
    sessions: Sessions,
    apartment: Apartment,
}

impl Backend {
    pub fn new() -> Self {
        let apartment = Apartment::join_multithreaded();
        Self { endpoints: Endpoints::default(), sessions: Sessions::default(), apartment }
    }

    pub fn is_supported(&self) -> bool {
        self.apartment.is_joined()
    }

    pub fn unsupported_reason(&self) -> Option<&str> {
        (!self.is_supported()).then_some(COM_UNAVAILABLE)
    }

    pub fn apps(&self) -> &[AudioApp] {
        self.sessions.apps()
    }

    pub fn master(&self) -> Option<&AudioApp> {
        self.endpoints.master()
    }

    pub fn input(&self) -> Option<&AudioApp> {
        self.endpoints.input()
    }

    pub fn devices(&self, kind: DeviceKind) -> &[AudioDevice] {
        self.endpoints.devices(kind)
    }

    pub fn refresh(&mut self) {
        if !self.is_supported() {
            return;
        }
        let output = self.endpoints.refresh();
        let listed = output.is_some_and(|device| self.sessions.refresh(&device));
        if !listed {
            self.sessions.clear();
        }
    }

    pub fn update_levels(&mut self) {
        self.sessions.update_levels();
        self.endpoints.update_levels();
    }

    pub fn set_volume(&mut self, id: AppId, volume: f32) {
        match DeviceKind::for_row(id) {
            Some(kind) => self.endpoints.endpoint_mut(kind).set_volume(volume),
            None => self.sessions.set_volume(id, volume),
        }
    }

    pub fn set_muted(&mut self, id: AppId, muted: bool) {
        match DeviceKind::for_row(id) {
            Some(kind) => self.endpoints.endpoint_mut(kind).set_muted(muted),
            None => self.sessions.set_muted(id, muted),
        }
    }

    pub fn set_default_device(&mut self, kind: DeviceKind, id: &str) {
        if self.endpoints.set_default(kind, id) {
            self.refresh();
        }
    }
}

impl Drop for Backend {
    fn drop(&mut self) {
        self.sessions.clear();
        self.endpoints.release();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::{INPUT, MASTER};

    #[test]
    fn enumerates_without_panicking() {
        let mut backend = Backend::new();
        assert!(backend.is_supported());
        backend.refresh();
        backend.update_levels();
        for row in [backend.master(), backend.input()].into_iter().flatten() {
            assert!((0.0..=1.0).contains(&row.volume));
            assert!(!row.name.is_empty());
        }
        for app in backend.apps() {
            assert!(app.id != MASTER && app.id != INPUT);
            assert!(!app.name.is_empty());
            assert!((0.0..=1.0).contains(&app.volume));
        }
        for kind in [DeviceKind::Output, DeviceKind::Input] {
            let devices = backend.devices(kind);
            assert!(devices.iter().filter(|d| d.is_default).count() <= 1);
            assert!(devices.iter().all(|d| d.kind == kind && !d.id.is_empty()));
        }
    }
}
