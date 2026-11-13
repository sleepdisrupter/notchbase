//! The default output or input device's volume, mute and peak.

use ::windows::Win32::Media::Audio::Endpoints::{IAudioEndpointVolume, IAudioMeterInformation};
use ::windows::Win32::Media::Audio::IMMDevice;
use ::windows::Win32::System::Com::CLSCTX_ALL;

use super::com::EVENT_CONTEXT;
use super::device_labels::{DeviceLabels, device_id};
use crate::audio::{AudioApp, DeviceKind};

#[derive(Default)]
pub struct DefaultEndpoint {
    device_id: String,
    volume: Option<IAudioEndpointVolume>,
    meter: Option<IAudioMeterInformation>,
    row: Option<AudioApp>,
}

impl DefaultEndpoint {
    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    pub fn row(&self) -> Option<&AudioApp> {
        self.row.as_ref()
    }

    pub fn update(&mut self, device: Option<&IMMDevice>, kind: DeviceKind, labels: &mut DeviceLabels) {
        let Some(device) = device else {
            *self = Self::default();
            return;
        };
        let device_id = device_id(device).unwrap_or_default();
        if self.volume.is_none() || self.device_id != device_id {
            self.activate(device);
            self.device_id = device_id;
        }
        let Some((level, muted)) = self.volume.as_ref().and_then(read_levels) else {
            self.deactivate();
            return;
        };
        let name =
            labels.get(device, &self.device_id).map_or_else(|| kind.fallback_name().to_owned(), |label| label.name);
        let peak = self.row.as_ref().map_or(0.0, |row| row.peak);
        self.row = Some(AudioApp { id: kind.row_id(), name, volume: level, muted, peak, icon: None });
    }

    pub fn update_levels(&mut self) {
        let Some(row) = &mut self.row else { return };
        if let Some((level, muted)) = self.volume.as_ref().and_then(read_levels) {
            row.volume = level;
            row.muted = muted;
        }
        if let Some(meter) = &self.meter {
            row.peak = unsafe { meter.GetPeakValue() }.unwrap_or(0.0).clamp(0.0, 1.0);
        }
    }

    pub fn set_volume(&mut self, level: f32) {
        if let (Some(row), Some(volume)) = (&mut self.row, &self.volume)
            && unsafe { volume.SetMasterVolumeLevelScalar(level, &EVENT_CONTEXT) }.is_ok()
        {
            row.volume = level;
        }
    }

    pub fn set_muted(&mut self, muted: bool) {
        if let (Some(row), Some(volume)) = (&mut self.row, &self.volume)
            && unsafe { volume.SetMute(muted, &EVENT_CONTEXT) }.is_ok()
        {
            row.muted = muted;
        }
    }

    fn activate(&mut self, device: &IMMDevice) {
        self.volume = unsafe { device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None) }.ok();
        self.meter = unsafe { device.Activate::<IAudioMeterInformation>(CLSCTX_ALL, None) }.ok();
    }

    fn deactivate(&mut self) {
        self.volume = None;
        self.meter = None;
        self.row = None;
    }
}

fn read_levels(volume: &IAudioEndpointVolume) -> Option<(f32, bool)> {
    let level = unsafe { volume.GetMasterVolumeLevelScalar() }.ok()?;
    let muted = unsafe { volume.GetMute() }.is_ok_and(|m| m.as_bool());
    Some((level.clamp(0.0, 1.0), muted))
}
