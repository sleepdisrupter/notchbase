//! Devices from Core Audio, leaving out our own mixer devices. Picking an output also moves alerts and sound effects.

use std::collections::HashMap;

use super::endpoint::Endpoint;
use super::property::{AudioObjectID, PropertyAddress, read, read_ids, read_string, write};
use super::selectors::{
    DEVICE_DATA_SOURCE, DEVICE_TRANSPORT, DEVICE_UID, HW_DEFAULT_INPUT, HW_DEFAULT_OUTPUT, HW_DEFAULT_SYSTEM_OUTPUT,
    HW_DEVICES, OBJECT_NAME, SYSTEM_OBJECT, device_scope,
};
use super::streams::stream_count;
use super::taps::is_mixer_aggregate;
use crate::audio::core_audio::transport::{self, device_form};
use crate::audio::per_kind::PerKind;
use crate::audio::sorting::sort_devices;
use crate::audio::{AudioApp, AudioDevice, DeviceKind};

#[derive(Clone, Copy)]
pub struct Output<'a> {
    pub device: AudioObjectID,
    pub uid: &'a str,
}

#[derive(Default)]
pub struct Devices {
    endpoints: PerKind<Option<Endpoint>>,
    lists: PerKind<Vec<AudioDevice>>,
    output_uid: String,
    ids: HashMap<String, AudioObjectID>,
}

impl Devices {
    pub fn master(&self) -> Option<&AudioApp> {
        self.endpoints.get(DeviceKind::Output).as_ref().map(Endpoint::row)
    }

    pub fn input(&self) -> Option<&AudioApp> {
        self.endpoints.get(DeviceKind::Input).as_ref().map(Endpoint::row)
    }

    pub fn devices(&self, kind: DeviceKind) -> &[AudioDevice] {
        self.lists.get(kind)
    }

    pub fn output(&self) -> Option<Output<'_>> {
        let endpoint = self.endpoints.get(DeviceKind::Output).as_ref()?;
        Some(Output { device: endpoint.device(), uid: &self.output_uid })
    }

    pub fn sync_levels(&mut self) {
        for kind in DeviceKind::ALL {
            if let Some(endpoint) = self.endpoints.get_mut(kind) {
                endpoint.sync_levels();
            }
        }
    }

    pub fn set_volume(&mut self, kind: DeviceKind, volume: f32) {
        if let Some(endpoint) = self.endpoints.get_mut(kind) {
            endpoint.set_volume(volume);
        }
    }

    pub fn set_muted(&mut self, kind: DeviceKind, muted: bool) {
        if let Some(endpoint) = self.endpoints.get_mut(kind) {
            endpoint.set_muted(muted);
        }
    }

    pub fn make_default(&self, kind: DeviceKind, id: &str) -> bool {
        let Some(&device) = self.ids.get(id) else { return false };
        let made = write(SYSTEM_OBJECT, PropertyAddress::global(default_selector(kind)), device);
        if made && kind == DeviceKind::Output {
            write(SYSTEM_OBJECT, PropertyAddress::global(HW_DEFAULT_SYSTEM_OUTPUT), device);
        }
        made
    }

    pub fn refresh(&mut self) {
        let defaults = PerKind::from_fn(default_device);
        self.lists = PerKind::default();
        self.ids.clear();
        let mut known: HashMap<AudioObjectID, HalDevice> = HashMap::new();
        for object in read_ids(SYSTEM_OBJECT, PropertyAddress::global(HW_DEVICES)) {
            let Some(device) = HalDevice::read(object) else { continue };
            for kind in DeviceKind::ALL {
                if let Some(entry) = device.entry(kind, *defaults.get(kind)) {
                    self.lists.get_mut(kind).push(entry);
                }
            }
            self.ids.insert(device.uid.clone(), object);
            known.insert(object, device);
        }
        for kind in DeviceKind::ALL {
            sort_devices(self.lists.get_mut(kind));
        }
        let default_output = *defaults.get(DeviceKind::Output);
        self.output_uid = known.get(&default_output).map(|device| device.uid.clone()).unwrap_or_default();
        self.endpoints = PerKind::from_fn(|kind| {
            let device = *defaults.get(kind);
            let name = known.get(&device).map_or_else(|| kind.fallback_name().to_owned(), |known| known.name.clone());
            Endpoint::open(device, kind, name)
        });
    }
}

struct HalDevice {
    object: AudioObjectID,
    uid: String,
    name: String,
    transport: u32,
}

impl HalDevice {
    fn read(object: AudioObjectID) -> Option<Self> {
        let uid = read_string(object, PropertyAddress::global(DEVICE_UID))?;
        if is_mixer_aggregate(&uid) {
            return None;
        }
        let name = read_string(object, PropertyAddress::global(OBJECT_NAME)).unwrap_or_else(|| uid.clone());
        let transport = read::<u32>(object, PropertyAddress::global(DEVICE_TRANSPORT)).unwrap_or(0);
        Some(Self { object, uid, name, transport })
    }

    fn entry(&self, kind: DeviceKind, default: AudioObjectID) -> Option<AudioDevice> {
        let scope = device_scope(kind);
        if stream_count(self.object, scope) == 0 {
            return None;
        }
        let source = if self.transport == transport::BUILT_IN {
            read::<u32>(self.object, PropertyAddress::scoped(DEVICE_DATA_SOURCE, scope))
        } else {
            None
        };
        Some(AudioDevice {
            id: self.uid.clone(),
            name: self.name.clone(),
            kind,
            is_default: self.object == default,
            form: device_form(self.transport, source, kind),
        })
    }
}

fn default_selector(kind: DeviceKind) -> u32 {
    match kind {
        DeviceKind::Output => HW_DEFAULT_OUTPUT,
        DeviceKind::Input => HW_DEFAULT_INPUT,
    }
}

pub fn default_output() -> Option<(AudioObjectID, String)> {
    let device = default_device(DeviceKind::Output);
    let uid = read_string(device, PropertyAddress::global(DEVICE_UID))?;
    (device != 0 && !uid.is_empty()).then_some((device, uid))
}

pub fn default_input() -> Option<AudioObjectID> {
    Some(default_device(DeviceKind::Input)).filter(|device| *device != 0)
}

fn default_device(kind: DeviceKind) -> AudioObjectID {
    read::<AudioObjectID>(SYSTEM_OBJECT, PropertyAddress::global(default_selector(kind))).unwrap_or(0)
}
