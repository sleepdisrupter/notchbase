//! The default output device's volume and mute on macOS.

use objc2::rc::autoreleasepool;

use crate::audio::master::{output_state, set_output_muted, set_output_volume};
use crate::system::volume::steps::Change;
use crate::system::volume::{Level, Reading};

pub struct Output;

impl Output {
    pub fn open() -> Self {
        Self
    }

    pub fn read(&mut self) -> Option<Reading> {
        autoreleasepool(|_| {
            let state = output_state()?;
            Some(Reading { device: state.device, level: Level { volume: state.volume, muted: state.muted } })
        })
    }

    pub fn apply(&mut self, change: Change) {
        if let Some(volume) = change.volume {
            set_output_volume(volume);
        }
        if let Some(muted) = change.muted {
            set_output_muted(muted);
        }
    }
}
