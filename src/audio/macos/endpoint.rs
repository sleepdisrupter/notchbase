//! The default output or input: its volume from the main control, or the average of left and right, and its mute.

use super::property::{AudioObjectID, PropertyAddress, has, read, settable, write};
use super::selectors::{DEVICE_MUTE, DEVICE_VIRTUAL_MAIN_VOLUME, DEVICE_VOLUME, device_scope};
use crate::audio::{AudioApp, DeviceKind};

const MAIN_VOLUME_SELECTORS: [u32; 2] = [DEVICE_VIRTUAL_MAIN_VOLUME, DEVICE_VOLUME];
const STEREO_ELEMENTS: [u32; 2] = [1, 2];

pub struct Endpoint {
    device: AudioObjectID,
    scope: u32,
    row: AudioApp,
}

impl Endpoint {
    pub fn open(device: AudioObjectID, kind: DeviceKind, name: String) -> Option<Self> {
        if device == 0 {
            return None;
        }
        let scope = device_scope(kind);
        let row = AudioApp {
            id: kind.row_id(),
            name,
            volume: read_volume(device, scope).unwrap_or(1.0),
            muted: read_mute(device, scope).unwrap_or(false),
            peak: 0.0,
            icon: None,
        };
        Some(Self { device, scope, row })
    }

    pub fn device(&self) -> AudioObjectID {
        self.device
    }

    pub fn row(&self) -> &AudioApp {
        &self.row
    }

    pub fn sync_levels(&mut self) {
        if let Some(volume) = read_volume(self.device, self.scope) {
            self.row.volume = volume;
        }
        if let Some(muted) = read_mute(self.device, self.scope) {
            self.row.muted = muted;
        }
    }

    pub fn set_volume(&mut self, volume: f32) {
        if write_volume(self.device, self.scope, volume) {
            self.row.volume = volume;
        }
    }

    pub fn set_muted(&mut self, muted: bool) {
        if write_mute(self.device, self.scope, muted) {
            self.row.muted = muted;
        }
    }
}

pub(super) fn read_volume(device: AudioObjectID, scope: u32) -> Option<f32> {
    for selector in MAIN_VOLUME_SELECTORS {
        let address = PropertyAddress::scoped(selector, scope);
        if has(device, address)
            && let Some(volume) = read::<f32>(device, address)
        {
            return Some(volume.clamp(0.0, 1.0));
        }
    }
    let channels: Vec<f32> = STEREO_ELEMENTS
        .into_iter()
        .map(|element| PropertyAddress::element(DEVICE_VOLUME, scope, element))
        .filter(|&address| has(device, address))
        .filter_map(|address| read::<f32>(device, address))
        .collect();
    (!channels.is_empty()).then(|| (channels.iter().sum::<f32>() / channels.len() as f32).clamp(0.0, 1.0))
}

pub(super) fn write_volume(device: AudioObjectID, scope: u32, volume: f32) -> bool {
    for selector in MAIN_VOLUME_SELECTORS {
        let address = PropertyAddress::scoped(selector, scope);
        if has(device, address) && settable(device, address) {
            return write(device, address, volume);
        }
    }
    let mut any = false;
    for element in STEREO_ELEMENTS {
        let address = PropertyAddress::element(DEVICE_VOLUME, scope, element);
        if has(device, address) && settable(device, address) {
            any |= write(device, address, volume);
        }
    }
    any
}

pub(super) fn read_mute(device: AudioObjectID, scope: u32) -> Option<bool> {
    let address = PropertyAddress::scoped(DEVICE_MUTE, scope);
    if has(device, address) { read::<u32>(device, address).map(|m| m != 0) } else { None }
}

pub(super) fn write_mute(device: AudioObjectID, scope: u32, muted: bool) -> bool {
    let address = PropertyAddress::scoped(DEVICE_MUTE, scope);
    has(device, address) && settable(device, address) && write(device, address, muted as u32)
}
