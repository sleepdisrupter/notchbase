//! The recorder's settings, and the options they make for the host's recorder.

use notchbase_sdk::prelude::*;

pub const MICROPHONE: &str = "microphone";
pub const DESKTOP_AUDIO: &str = "desktop_audio";
pub const MICROPHONE_ID: &str = "microphone_id";
const FOLDER: &str = "folder";
const FRAME_RATE: &str = "frame_rate";
const DIM: &str = "dim_outside";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    pub microphone: bool,
    pub desktop_audio: bool,
    pub microphone_id: Option<String>,
    folder: Option<String>,
    frame_rate: Option<u32>,
    dim_outside: bool,
}

impl Options {
    pub fn declare(manifest: Manifest) -> Manifest {
        manifest
            // These four are set from the tile, so they stay off the settings page.
            .setting(SettingSpec::toggle(MICROPHONE, "Record the microphone", false).hidden())
            .setting(SettingSpec::toggle(DESKTOP_AUDIO, "Record desktop audio", false).hidden())
            .setting(SettingSpec::text(MICROPHONE_ID, "Microphone", "").hidden())
            .setting(SettingSpec::text(FOLDER, "Recording folder", "").placeholder("Videos").hidden())
            .setting(
                SettingSpec::choice(FRAME_RATE, "Frame rate", [("auto", "Auto"), ("60", "60"), ("30", "30")], "auto")
                    .detail("Auto records 60 fps, or 30 on very large areas"),
            )
            .setting(
                SettingSpec::toggle(DIM, "Dim around recordings", true).detail("Shade the screen outside the area"),
            )
    }

    pub fn from(settings: &Settings) -> Self {
        Self {
            microphone: settings.bool(MICROPHONE),
            desktop_audio: settings.bool(DESKTOP_AUDIO),
            microphone_id: settings.string(MICROPHONE_ID),
            folder: settings.string(FOLDER),
            frame_rate: settings.text(FRAME_RATE).parse().ok(), // "auto" doesn't parse, which leaves it to the host
            dim_outside: settings.get(DIM).and_then(Value::as_bool).unwrap_or(true),
        }
    }

    pub fn recording(&self) -> RecordingOptions {
        RecordingOptions {
            microphone: self.microphone,
            desktop_audio: self.desktop_audio,
            microphone_id: self.microphone_id.clone(),
            folder: self.folder.clone(),
            max_fps: self.frame_rate,
            dim_outside: self.dim_outside,
        }
    }
}
