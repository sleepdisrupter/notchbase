//! On macOS, the camera from CoreMediaIO, the apps recording audio, and the input mute.

mod camera;

use notchbase_sdk::protocol::Sensors;
use objc2::rc::autoreleasepool;

use crate::audio::master::{input_muted, set_input_muted};
use crate::audio::recording_apps;

pub(super) struct Probe;

impl Probe {
    pub(super) fn open() -> Self {
        Self
    }

    pub(super) fn read(&mut self) -> Sensors {
        autoreleasepool(|_| {
            let apps = recording_apps();
            Sensors {
                camera: camera::running(),
                microphone: !apps.is_empty(),
                apps,
                muted: input_muted().unwrap_or(false),
            }
        })
    }

    pub(super) fn set_muted(&mut self, muted: bool) {
        set_input_muted(muted);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "asks this Mac which cameras and microphones are in use"]
    fn camera_and_mic_can_be_read() {
        assert!(camera::devices().is_some(), "the camera list can be read");
        eprintln!("{:?}", Probe::open().read());
    }
}
