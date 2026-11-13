//! The default output device's volume and mute through its audio endpoint, following a change of default device.

use std::ptr::null;
use std::time::{Duration, Instant};

use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, eConsole, eRender};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance, CoTaskMemFree};

use crate::platform::Apartment;
use crate::system::volume::steps::Change;
use crate::system::volume::{Level, Reading};

const RECHECK_DEVICE: Duration = Duration::from_millis(500);

pub struct Output {
    enumerator: Option<IMMDeviceEnumerator>,
    endpoint: Option<(String, IAudioEndpointVolume)>,
    checked_at: Option<Instant>,
    _apartment: Apartment,
}

impl Output {
    pub fn open() -> Self {
        let apartment = Apartment::join();
        Self { enumerator: None, endpoint: None, checked_at: None, _apartment: apartment }
    }

    pub fn read(&mut self) -> Option<Reading> {
        self.follow_default();
        let (device, volume) = self.endpoint.as_ref()?;
        let level = unsafe { volume.GetMasterVolumeLevelScalar() }.ok()?;
        let muted = unsafe { volume.GetMute() }.is_ok_and(|m| m.as_bool());
        Some(Reading { device: device.clone(), level: Level { volume: level.clamp(0.0, 1.0), muted } })
    }

    pub fn apply(&mut self, change: Change) {
        let Some((_, volume)) = &self.endpoint else { return };
        unsafe {
            if let Some(level) = change.volume {
                let _ = volume.SetMasterVolumeLevelScalar(level, null());
            }
            if let Some(muted) = change.muted {
                let _ = volume.SetMute(muted, null());
            }
        }
    }

    fn follow_default(&mut self) {
        let now = Instant::now();
        if self.checked_at.is_some_and(|at| now.duration_since(at) < RECHECK_DEVICE) && self.endpoint.is_some() {
            return;
        }
        self.checked_at = Some(now);
        if self.enumerator.is_none() {
            self.enumerator = unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }.ok();
        }
        let Some(enumerator) = &self.enumerator else { return };
        let Ok(device) = (unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eConsole) }) else {
            self.endpoint = None;
            return;
        };
        let Some(id) = device_id(&device) else { return };
        if self.endpoint.as_ref().is_some_and(|(current, _)| *current == id) {
            return;
        }
        self.endpoint = unsafe { device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None) }.ok().map(|v| (id, v));
    }
}

fn device_id(device: &IMMDevice) -> Option<String> {
    let raw = unsafe { device.GetId() }.ok()?;
    let id = unsafe { raw.to_string() }.ok();
    unsafe { CoTaskMemFree(Some(raw.0 as *const _)) };
    id
}
