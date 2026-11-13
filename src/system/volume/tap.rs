//! Which keys the notch has claimed: volume keys while its indicator is on, media keys while something plays too, and
//! brightness keys on their own. The rest go through to the system.

use std::sync::atomic::{AtomicBool, Ordering};

use super::{BrightnessPress, Key, KeyPress, MediaKey, Press};

#[derive(Default)]
pub struct TapState {
    volume: AtomicBool,
    media: AtomicBool,
    brightness: AtomicBool,
}

impl TapState {
    pub fn claim_volume(&self, on: bool) {
        self.volume.store(on, Ordering::Relaxed);
    }

    pub fn claim_media(&self, on: bool) {
        self.media.store(on, Ordering::Relaxed);
    }

    pub fn claim_brightness(&self, on: bool) {
        self.brightness.store(on, Ordering::Relaxed);
    }

    pub fn wants(&self, key: Option<Key>) -> Option<Key> {
        let claimed = |claim: &AtomicBool| claim.load(Ordering::Relaxed);
        let key = key?;
        let wanted = match key {
            Key::Volume(_) => claimed(&self.volume),
            Key::Media(_) => claimed(&self.volume) && claimed(&self.media),
            Key::Brightness(_) => claimed(&self.brightness),
        };
        wanted.then_some(key)
    }
}

pub fn volume(press: Press, fine: bool) -> Key {
    Key::Volume(KeyPress { press, fine })
}

pub fn media(key: MediaKey) -> Key {
    Key::Media(key)
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn brightness(up: bool, fine: bool) -> Key {
    Key::Brightness(BrightnessPress { up, fine })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_keys_follow_the_indicator() {
        let tap = TapState::default();
        let mute = volume(Press::Mute, false);
        assert_eq!(tap.wants(Some(mute)), None);
        tap.claim_volume(true);
        assert_eq!(tap.wants(Some(mute)), Some(mute));
        assert_eq!(tap.wants(None), None);
    }

    #[test]
    fn media_keys_only_while_playing() {
        let tap = TapState::default();
        let play = media(MediaKey::PlayPause);
        tap.claim_volume(true);
        assert_eq!(tap.wants(Some(play)), None);
        tap.claim_media(true);
        assert_eq!(tap.wants(Some(play)), Some(play));
        tap.claim_volume(false);
        assert_eq!(tap.wants(Some(play)), None, "they belong to the volume indicator");
    }

    #[test]
    fn brightness_keys_stand_alone() {
        let tap = TapState::default();
        let up = brightness(true, false);
        assert_eq!(tap.wants(Some(up)), None);
        tap.claim_brightness(true);
        assert_eq!(tap.wants(Some(up)), Some(up));
        assert_eq!(tap.wants(Some(volume(Press::Up, false))), None);
    }
}
