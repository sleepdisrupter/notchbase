//! Whether System Audio Recording is allowed, read without asking.

use crate::system::permissions::Standing;
use crate::system::permissions::macos::tcc;

const AUDIO_CAPTURE_SERVICE: &str = "kTCCServiceAudioCapture";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Permission {
    Granted,
    Denied,
    Unknown,
}

pub fn audio_capture_permission() -> Permission {
    match tcc::preflight(AUDIO_CAPTURE_SERVICE) {
        Some(Standing::Granted) => Permission::Granted,
        Some(Standing::Denied) => Permission::Denied,
        _ => Permission::Unknown,
    }
}
