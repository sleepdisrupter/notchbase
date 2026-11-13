//! Active output and input devices, and the default of each.

use ::windows::Win32::Media::Audio::{
    DEVICE_STATE_ACTIVE, EDataFlow, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, eCapture, eConsole, eRender,
};
use ::windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance};

use super::default_endpoint::DefaultEndpoint;
use super::device_labels::{DeviceLabels, device_id};
use super::policy_config::set_default_endpoint;
use crate::audio::per_kind::PerKind;
use crate::audio::sorting::sort_devices;
use crate::audio::{AudioApp, AudioDevice, DeviceKind};

#[derive(Default)]
pub struct Endpoints {
    enumerator: Option<IMMDeviceEnumerator>,
    defaults: PerKind<DefaultEndpoint>,
    lists: PerKind<Vec<AudioDevice>>,
    labels: DeviceLabels,
}

impl Endpoints {
    pub fn master(&self) -> Option<&AudioApp> {
        self.defaults.get(DeviceKind::Output).row()
    }

    pub fn input(&self) -> Option<&AudioApp> {
        self.defaults.get(DeviceKind::Input).row()
    }

    pub fn devices(&self, kind: DeviceKind) -> &[AudioDevice] {
        self.lists.get(kind)
    }

    pub fn endpoint_mut(&mut self, kind: DeviceKind) -> &mut DefaultEndpoint {
        self.defaults.get_mut(kind)
    }

    pub fn refresh(&mut self) -> Option<IMMDevice> {
        if self.enumerator.is_none() {
            self.enumerator = unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }.ok();
        }
        let Some(enumerator) = &self.enumerator else {
            self.clear();
            return None;
        };
        let current = PerKind::from_fn(|kind| default_device(enumerator, kind));
        for kind in DeviceKind::ALL {
            let endpoint = self.defaults.get_mut(kind);
            endpoint.update(current.get(kind).as_ref(), kind, &mut self.labels);
            *self.lists.get_mut(kind) = list_devices(enumerator, kind, endpoint.device_id(), &mut self.labels);
        }
        // With no devices at all the enumerator may be stale, so the next refresh makes a fresh one.
        if self.found_nothing(&current) {
            self.enumerator = None;
        }
        current.get(DeviceKind::Output).clone()
    }

    pub fn update_levels(&mut self) {
        for kind in DeviceKind::ALL {
            self.defaults.get_mut(kind).update_levels();
        }
    }

    pub fn set_default(&self, kind: DeviceKind, id: &str) -> bool {
        self.devices(kind).iter().any(|d| d.id == id) && set_default_endpoint(id)
    }

    pub fn release(&mut self) {
        self.clear();
        self.enumerator = None;
    }

    fn found_nothing(&self, current: &PerKind<Option<IMMDevice>>) -> bool {
        DeviceKind::ALL.into_iter().all(|kind| current.get(kind).is_none() && self.lists.get(kind).is_empty())
    }

    fn clear(&mut self) {
        self.defaults = PerKind::default();
        self.lists = PerKind::default();
    }
}

fn data_flow(kind: DeviceKind) -> EDataFlow {
    match kind {
        DeviceKind::Output => eRender,
        DeviceKind::Input => eCapture,
    }
}

fn default_device(enumerator: &IMMDeviceEnumerator, kind: DeviceKind) -> Option<IMMDevice> {
    unsafe { enumerator.GetDefaultAudioEndpoint(data_flow(kind), eConsole) }.ok()
}

fn list_devices(
    enumerator: &IMMDeviceEnumerator,
    kind: DeviceKind,
    default_id: &str,
    labels: &mut DeviceLabels,
) -> Vec<AudioDevice> {
    let Ok(collection) = (unsafe { enumerator.EnumAudioEndpoints(data_flow(kind), DEVICE_STATE_ACTIVE) }) else {
        return Vec::new();
    };
    let count = unsafe { collection.GetCount() }.unwrap_or(0);
    let mut devices: Vec<AudioDevice> = (0..count)
        .filter_map(|i| unsafe { collection.Item(i) }.ok())
        .filter_map(|device| {
            let id = device_id(&device)?;
            let label = labels.get(&device, &id)?;
            Some(AudioDevice { is_default: id == default_id, id, name: label.name, kind, form: label.form })
        })
        .collect();
    sort_devices(&mut devices);
    devices
}
