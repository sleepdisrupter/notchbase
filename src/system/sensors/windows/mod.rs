//! On Windows, the apps using the camera and microphone from the consent store, and the default microphone's mute.

mod consent;
mod microphone;

use notchbase_sdk::protocol::Sensors;

use self::microphone::Microphone;
use super::names::apps;

const CAMERA: &str = "webcam";
const MICROPHONE: &str = "microphone";

pub(super) struct Probe {
    microphone: Microphone,
    me: String,
}

impl Probe {
    pub(super) fn open() -> Self {
        let me = std::env::current_exe().ok().and_then(|path| Some(path.file_stem()?.to_string_lossy().into_owned()));
        Self { microphone: Microphone::open(), me: me.unwrap_or_default() }
    }

    pub(super) fn read(&mut self) -> Sensors {
        let cameras = apps(consent::users(CAMERA), &self.me);
        let microphones = apps(consent::users(MICROPHONE), &self.me);
        let mut apps = microphones.clone();
        apps.extend(cameras.iter().filter(|name| !microphones.contains(name)).cloned());
        Sensors {
            camera: !cameras.is_empty(),
            microphone: !microphones.is_empty(),
            apps,
            muted: self.microphone.muted(),
        }
    }

    pub(super) fn set_muted(&mut self, muted: bool) {
        self.microphone.set_muted(muted);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "asks this computer which apps are using the camera and microphone"]
    fn camera_and_mic_can_be_read() {
        eprintln!("{:?}", Probe::open().read());
    }
}
