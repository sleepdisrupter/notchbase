//! The macOS backend: devices, app rows and taps kept in step. Taps need macOS 14.2 and System Audio Recording access.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use objc2::rc::autoreleasepool;

use super::apps::AppList;
use super::devices::Devices;
use super::taps::{Mix, Taps};
use crate::audio::{AppId, AudioApp, AudioDevice, DeviceKind};

const MAINTAIN_EVERY: Duration = Duration::from_secs(1);

pub struct Backend {
    devices: Devices,
    apps: AppList,
    taps: Taps,
    refreshed_at: Option<Instant>,
}

impl Backend {
    pub fn new() -> Self {
        Self { devices: Devices::default(), apps: AppList::default(), taps: Taps::new(), refreshed_at: None }
    }

    pub fn is_supported(&self) -> bool {
        self.taps.is_available() && !self.taps.is_denied()
    }

    pub fn unsupported_reason(&self) -> Option<&str> {
        if !self.taps.is_available() {
            Some("Per-app volume needs macOS 14.2 or later")
        } else if self.taps.is_denied() {
            Some("Allow System Audio Recording for notchbase in System Settings")
        } else {
            None
        }
    }

    pub fn apps(&self) -> &[AudioApp] {
        self.apps.rows()
    }

    pub fn master(&self) -> Option<&AudioApp> {
        self.devices.master()
    }

    pub fn input(&self) -> Option<&AudioApp> {
        self.devices.input()
    }

    pub fn devices(&self, kind: DeviceKind) -> &[AudioDevice] {
        self.devices.devices(kind)
    }

    pub fn refresh(&mut self) {
        self.refreshed_at = Some(Instant::now());
        autoreleasepool(|_| {
            self.devices.refresh();
            if !self.taps.is_available() {
                return;
            }
            self.taps.refresh_permission();
            if self.taps.is_denied() {
                self.taps.reset();
                self.apps.clear();
                return;
            }
            self.apps.refresh(&self.taps);
            self.sync_taps();
        });
    }

    /// Refreshes once a second while any tap is open, even with no page showing, so taps follow an app's new processes.
    pub fn maintain(&mut self) {
        let stale = self.refreshed_at.is_none_or(|at| at.elapsed() >= MAINTAIN_EVERY);
        if stale && self.taps.has_active() {
            self.refresh();
        }
    }

    pub fn update_levels(&mut self) {
        self.devices.sync_levels();
        self.apps.update_levels(|id| self.taps.take_peak(id));
    }

    pub fn set_volume(&mut self, id: AppId, volume: f32) {
        match DeviceKind::for_row(id) {
            Some(kind) => self.devices.set_volume(kind, volume),
            None => self.set_app(id, Some(volume), None),
        }
    }

    pub fn set_muted(&mut self, id: AppId, muted: bool) {
        match DeviceKind::for_row(id) {
            Some(kind) => self.devices.set_muted(kind, muted),
            None => self.set_app(id, None, Some(muted)),
        }
    }

    pub fn set_default_device(&mut self, kind: DeviceKind, id: &str) {
        if self.devices.make_default(kind, id) {
            self.refresh();
        }
    }

    fn set_app(&mut self, id: AppId, volume: Option<f32>, muted: Option<bool>) {
        let Some(i) = self.apps.position(id) else { return };
        let mix = self.taps.desire(id, self.apps.mix(i), volume, muted);
        self.apps.set_mix(i, mix);
        if !self.taps.retune(id, mix) && mix.needs_tap() {
            self.start_tap(i);
        }
    }

    fn start_tap(&mut self, i: usize) {
        let started = self.taps.start(self.apps.id(i), self.apps.owner(i), self.devices.output());
        if !started {
            self.apps.set_mix(i, Mix::UNITY);
        }
    }

    fn sync_taps(&mut self) {
        let Some(output) = self.devices.output() else {
            self.taps.close_all();
            return;
        };
        let listed: HashSet<AppId> = self.apps.rows().iter().map(|row| row.id).collect();
        self.taps.forget_unlisted(&listed);
        for i in 0..self.apps.rows().len() {
            let (id, owner) = (self.apps.id(i), self.apps.owner(i));
            self.taps.drop_unneeded(id, owner, output.uid);
            if self.taps.is_missing(id) && !self.taps.start(id, owner, Some(output)) {
                self.apps.set_mix(i, Mix::UNITY);
            }
        }
    }
}
