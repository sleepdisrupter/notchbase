//! The default output's volume and mute, and the default input's mute, for the volume keys and the microphone switch.

use super::devices::{default_input, default_output};
use super::endpoint::{read_mute, read_volume, write_mute, write_volume};
use super::selectors::{SCOPE_INPUT, SCOPE_OUTPUT};

pub struct OutputState {
    pub device: String,
    pub volume: f32,
    pub muted: bool,
}

pub fn output_state() -> Option<OutputState> {
    let (device, uid) = default_output()?;
    let volume = read_volume(device, SCOPE_OUTPUT)?;
    let muted = read_mute(device, SCOPE_OUTPUT).unwrap_or(false);
    Some(OutputState { device: uid, volume, muted })
}

pub fn set_output_volume(volume: f32) -> bool {
    default_output().is_some_and(|(device, _)| write_volume(device, SCOPE_OUTPUT, volume.clamp(0.0, 1.0)))
}

pub fn set_output_muted(muted: bool) -> bool {
    default_output().is_some_and(|(device, _)| write_mute(device, SCOPE_OUTPUT, muted))
}

pub fn input_muted() -> Option<bool> {
    read_mute(default_input()?, SCOPE_INPUT)
}

pub fn set_input_muted(muted: bool) -> bool {
    default_input().is_some_and(|device| write_mute(device, SCOPE_INPUT, muted))
}
