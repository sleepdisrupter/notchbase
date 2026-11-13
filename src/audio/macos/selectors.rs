//! Core Audio property selectors and scopes, spelled as four-character codes.

use super::property::AudioObjectID;
use crate::audio::DeviceKind;
use crate::audio::core_audio::fourcc;

pub const SYSTEM_OBJECT: AudioObjectID = 1;
pub const ELEMENT_MAIN: u32 = 0;

pub const SCOPE_GLOBAL: u32 = fourcc(b"glob");
pub const SCOPE_INPUT: u32 = fourcc(b"inpt");
pub const SCOPE_OUTPUT: u32 = fourcc(b"outp");

pub const HW_DEVICES: u32 = fourcc(b"dev#");
pub const HW_DEFAULT_INPUT: u32 = fourcc(b"dIn ");
pub const HW_DEFAULT_OUTPUT: u32 = fourcc(b"dOut");
pub const HW_DEFAULT_SYSTEM_OUTPUT: u32 = fourcc(b"sOut");
pub const HW_PROCESS_OBJECTS: u32 = fourcc(b"prs#");

pub const PROCESS_PID: u32 = fourcc(b"ppid");
pub const PROCESS_BUNDLE_ID: u32 = fourcc(b"pbid");
pub const PROCESS_RUNNING_OUTPUT: u32 = fourcc(b"piro");
pub const PROCESS_RUNNING_INPUT: u32 = fourcc(b"piri");

pub const TAP_UID: u32 = fourcc(b"tuid");
pub const TAP_FORMAT: u32 = fourcc(b"tfmt");

pub const OBJECT_NAME: u32 = fourcc(b"lnam");
pub const DEVICE_UID: u32 = fourcc(b"uid ");
pub const DEVICE_STREAMS: u32 = fourcc(b"stm#");
pub const DEVICE_STREAM_CONFIGURATION: u32 = fourcc(b"slay");
pub const DEVICE_TRANSPORT: u32 = fourcc(b"tran");
pub const DEVICE_DATA_SOURCE: u32 = fourcc(b"ssrc");
pub const DEVICE_VOLUME: u32 = fourcc(b"volm");
pub const DEVICE_VIRTUAL_MAIN_VOLUME: u32 = fourcc(b"vmvc");
pub const DEVICE_MUTE: u32 = fourcc(b"mute");
pub const DEVICE_NOMINAL_RATE: u32 = fourcc(b"nsrt");
pub const STREAM_VIRTUAL_FORMAT: u32 = fourcc(b"sfmt");

pub const fn device_scope(kind: DeviceKind) -> u32 {
    match kind {
        DeviceKind::Output => SCOPE_OUTPUT,
        DeviceKind::Input => SCOPE_INPUT,
    }
}
