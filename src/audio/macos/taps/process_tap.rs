//! A Core Audio process tap, destroyed when dropped.

use super::TapError;
use super::api::TapApi;
use super::description::TapDescription;
use crate::audio::macos::permission::{Permission, audio_capture_permission};
use crate::audio::macos::property::{AudioObjectID, PropertyAddress, read_string};
use crate::audio::macos::selectors::{TAP_FORMAT, TAP_UID};
use crate::audio::macos::streams::has_float32_format;

pub struct ProcessTap {
    api: TapApi,
    id: AudioObjectID,
}

impl ProcessTap {
    pub fn create(api: TapApi, description: &TapDescription) -> Result<Self, TapError> {
        let Some(id) = api.create_process_tap(description) else {
            return Err(match audio_capture_permission() {
                Permission::Denied => TapError::Permission,
                _ => TapError::Failed,
            });
        };
        let tap = Self { api, id };
        if !has_float32_format(tap.id, TAP_FORMAT) {
            return Err(TapError::Failed);
        }
        Ok(tap)
    }

    pub fn uid(&self) -> Option<String> {
        read_string(self.id, PropertyAddress::global(TAP_UID))
    }
}

impl Drop for ProcessTap {
    fn drop(&mut self) {
        self.api.destroy_process_tap(self.id);
    }
}
