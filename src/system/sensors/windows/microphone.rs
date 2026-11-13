//! The default microphone's mute, through its audio endpoint.

use std::ptr::null;

use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{IMMDeviceEnumerator, MMDeviceEnumerator, eCapture, eConsole};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance};

use crate::platform::Apartment;

pub(super) struct Microphone {
    enumerator: Option<IMMDeviceEnumerator>,
    _apartment: Apartment,
}

impl Microphone {
    pub(super) fn open() -> Self {
        let apartment = Apartment::join();
        Self { enumerator: None, _apartment: apartment }
    }

    pub(super) fn muted(&mut self) -> bool {
        let Some(volume) = self.endpoint() else { return false };
        unsafe { volume.GetMute() }.is_ok_and(|muted| muted.as_bool())
    }

    pub(super) fn set_muted(&mut self, muted: bool) {
        if let Some(volume) = self.endpoint() {
            let _ = unsafe { volume.SetMute(muted, null()) };
        }
    }

    fn endpoint(&mut self) -> Option<IAudioEndpointVolume> {
        if self.enumerator.is_none() {
            self.enumerator = unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }.ok();
        }
        let device = unsafe { self.enumerator.as_ref()?.GetDefaultAudioEndpoint(eCapture, eConsole) }.ok()?;
        unsafe { device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None) }.ok()
    }
}
