//! One audio session: its volume, mute, peak and icon.

use std::sync::Arc;

use ::windows::Win32::Media::Audio::Endpoints::IAudioMeterInformation;
use ::windows::Win32::Media::Audio::{IAudioSessionControl, ISimpleAudioVolume};
use ::windows::core::Interface;
use egui::ColorImage;

use super::com::{EVENT_CONTEXT, take_string};
use super::icon_location::parse_icon_location;
use super::icons::load_icon;

pub struct Session {
    volume: ISimpleAudioVolume,
    meter: Option<IAudioMeterInformation>,
}

pub struct Levels {
    pub volume: f32,
    pub muted: bool,
    pub peak: f32,
}

impl Session {
    pub fn open(control: &IAudioSessionControl) -> Option<(Self, Levels)> {
        let volume: ISimpleAudioVolume = control.cast().ok()?;
        let level = unsafe { volume.GetMasterVolume() }.ok()?;
        let muted = unsafe { volume.GetMute() }.is_ok_and(|m| m.as_bool());
        let session = Self { volume, meter: control.cast().ok() };
        let peak = session.peak().unwrap_or(0.0);
        Some((session, Levels { volume: level, muted, peak }))
    }

    pub fn levels(&self) -> Option<(f32, bool)> {
        let level = unsafe { self.volume.GetMasterVolume() }.ok()?;
        let muted = unsafe { self.volume.GetMute() }.is_ok_and(|m| m.as_bool());
        Some((level.clamp(0.0, 1.0), muted))
    }

    pub fn peak(&self) -> Option<f32> {
        self.meter.as_ref().and_then(|meter| unsafe { meter.GetPeakValue() }.ok())
    }

    pub fn set_volume(&self, volume: f32) -> bool {
        unsafe { self.volume.SetMasterVolume(volume, &EVENT_CONTEXT) }.is_ok()
    }

    pub fn set_muted(&self, muted: bool) -> bool {
        unsafe { self.volume.SetMute(muted, &EVENT_CONTEXT) }.is_ok()
    }
}

pub fn session_icon(control: &IAudioSessionControl) -> Option<Arc<ColorImage>> {
    let raw = unsafe { control.GetIconPath() }.ok()?;
    let location = unsafe { take_string(raw) }?;
    let (file, index) = parse_icon_location(&location)?;
    load_icon(&file, index)
}
