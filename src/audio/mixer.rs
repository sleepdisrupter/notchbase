//! `Mixer` is what the app holds. Changes show at once and go to the driver numbered, and a reading from the driver is
//! only taken once it includes the latest change, so a slider never snaps back.

use super::driver::{Command, Driver, Snapshot};
use super::{AppId, AudioApp, AudioDevice, DeviceKind};

pub struct Mixer {
    view: Snapshot,
    driver: Driver,
    sent: u64,
    changed: bool,
}

impl Default for Mixer {
    fn default() -> Self {
        Self::new()
    }
}

impl Mixer {
    pub fn new() -> Self {
        let (driver, view) = Driver::start();
        Self { view, driver, sent: 0, changed: true }
    }

    pub fn is_supported(&self) -> bool {
        self.view.supported
    }

    pub fn unsupported_reason(&self) -> Option<&str> {
        self.view.reason.as_deref()
    }

    pub fn refresh(&mut self) {
        self.driver.demand();
        self.pull();
    }

    pub fn update_levels(&mut self) {
        self.pull();
    }

    pub fn poll(&mut self) -> bool {
        self.driver.demand();
        self.pull();
        std::mem::take(&mut self.changed)
    }

    pub fn maintain(&mut self) {
        self.driver.maintain();
    }

    pub fn apps(&self) -> &[AudioApp] {
        &self.view.apps
    }

    pub fn master(&self) -> Option<&AudioApp> {
        self.view.master.as_ref()
    }

    pub fn input(&self) -> Option<&AudioApp> {
        self.view.input.as_ref()
    }

    pub fn devices(&self, kind: DeviceKind) -> &[AudioDevice] {
        self.view.devices.get(kind)
    }

    pub fn set_default_device(&mut self, kind: DeviceKind, id: &str) {
        for device in self.view.devices.get_mut(kind) {
            device.is_default = device.id == id;
        }
        self.send(Command::DefaultDevice(kind, id.to_owned()));
    }

    pub fn set_volume(&mut self, id: AppId, volume: f32) {
        if !volume.is_finite() {
            return;
        }
        let volume = volume.clamp(0.0, 1.0);
        if let Some(row) = self.view.row_mut(id) {
            row.volume = volume;
        }
        self.send(Command::Volume(id, volume));
    }

    pub fn set_muted(&mut self, id: AppId, muted: bool) {
        if let Some(row) = self.view.row_mut(id) {
            row.muted = muted;
        }
        self.send(Command::Muted(id, muted));
    }

    fn send(&mut self, command: Command) {
        self.sent += 1;
        self.changed = true;
        self.driver.send(command);
    }

    fn pull(&mut self) {
        if let Some(snapshot) = self.driver.take()
            && snapshot.seq >= self.sent
        {
            self.view = snapshot;
            self.changed = true;
        }
    }
}
