//! A short alert sound: Glass on a Mac, and the first alarm on Windows, or its notification sound if that's missing.
//! An alarm notice rings with the same sound for a few minutes: Glass again every couple of seconds on a Mac, the
//! alarm looped on Windows.

use std::time::{Duration, Instant};

/// How long an alarm makes a sound before it only shakes.
const RING_FOR: Duration = Duration::from_secs(5 * 60);
/// How often the Mac plays Glass again while an alarm rings. Windows loops its alarm instead.
const AGAIN_EVERY: Duration = Duration::from_millis(2000);

#[cfg(windows)]
fn alert_sound() -> (std::ffi::OsString, u32) {
    use windows_sys::Win32::Media::Audio::{SND_ALIAS, SND_FILENAME};

    let alarm = std::env::var_os("WINDIR")
        .map(|dir| std::path::PathBuf::from(dir).join("Media").join("Alarm01.wav"))
        .filter(|path| path.exists());
    match alarm {
        Some(path) => (path.into_os_string(), SND_FILENAME),
        None => ("SystemNotification".into(), SND_ALIAS),
    }
}

#[cfg(windows)]
fn play(looped: bool) {
    use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_LOOP, SND_NODEFAULT};

    let (name, kind) = alert_sound();
    let wide: Vec<u16> = std::os::windows::ffi::OsStrExt::encode_wide(name.as_os_str()).chain([0]).collect();
    let looping = if looped { SND_LOOP } else { 0 };
    unsafe { PlaySoundW(wide.as_ptr(), std::ptr::null_mut(), kind | SND_ASYNC | SND_NODEFAULT | looping) };
}

#[cfg(windows)]
pub fn chime() {
    play(false);
}

#[cfg(windows)]
fn sound(sound: Sound) {
    use windows_sys::Win32::Media::Audio::PlaySoundW;

    match sound {
        Sound::Start => play(true),
        Sound::Again => {}
        Sound::Stop => unsafe {
            PlaySoundW(std::ptr::null(), std::ptr::null_mut(), 0);
        },
    }
}

#[cfg(target_os = "macos")]
pub fn chime() {
    let _ = std::process::Command::new("/usr/bin/afplay").arg("/System/Library/Sounds/Glass.aiff").spawn();
}

#[cfg(target_os = "macos")]
fn sound(sound: Sound) {
    match sound {
        Sound::Start | Sound::Again => chime(),
        // Each Glass is over in under two seconds, so there's nothing left to stop.
        Sound::Stop => {}
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn chime() {}

#[cfg(not(any(windows, target_os = "macos")))]
fn sound(_: Sound) {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sound {
    Start,
    Again,
    Stop,
}

/// The sound of whatever alarm is up. The app tells it each frame whether one is.
#[derive(Debug, Default)]
pub struct Ringer {
    since: Option<Instant>,
    last: Option<Instant>,
}

impl Ringer {
    pub fn ring(&mut self, ringing: bool, now: Instant) {
        if let Some(next) = self.step(ringing, now) {
            sound(next);
        }
    }

    /// When the next sound is due, so the app wakes for it.
    pub fn wakeup(&self) -> Option<Instant> {
        let (since, last) = (self.since?, self.last?);
        Some((last + AGAIN_EVERY).min(since + RING_FOR))
    }

    fn step(&mut self, ringing: bool, now: Instant) -> Option<Sound> {
        match (ringing, self.since, self.last) {
            (false, None, _) => None,
            (false, Some(_), last) => {
                *self = Self::default();
                last.map(|_| Sound::Stop)
            }
            (true, None, _) => {
                (self.since, self.last) = (Some(now), Some(now));
                Some(Sound::Start)
            }
            (true, Some(_), None) => None,
            (true, Some(since), Some(_)) if now.saturating_duration_since(since) >= RING_FOR => {
                self.last = None;
                Some(Sound::Stop)
            }
            (true, Some(_), Some(last)) if now.saturating_duration_since(last) >= AGAIN_EVERY => {
                self.last = Some(now);
                Some(Sound::Again)
            }
            (true, Some(_), Some(_)) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(seconds: f32) -> Duration {
        Duration::from_secs_f32(seconds)
    }

    #[test]
    fn rings_again_until_it_tires() {
        let (mut ringer, t0) = (Ringer::default(), Instant::now());
        assert_eq!(ringer.step(false, t0), None, "nothing to stop");
        assert_eq!(ringer.step(true, t0), Some(Sound::Start));
        assert_eq!(ringer.step(true, t0 + s(1.0)), None);
        assert_eq!(ringer.wakeup(), Some(t0 + AGAIN_EVERY));
        assert_eq!(ringer.step(true, t0 + AGAIN_EVERY), Some(Sound::Again));
        assert_eq!(ringer.step(true, t0 + RING_FOR), Some(Sound::Stop));
        assert_eq!(ringer.wakeup(), None, "quiet, while the alarm still shows");
        assert_eq!(ringer.step(true, t0 + RING_FOR + s(9.0)), None);
        assert_eq!(ringer.step(false, t0 + RING_FOR + s(10.0)), None, "already quiet");
    }

    #[test]
    fn stops_when_the_alarm_goes() {
        let (mut ringer, t0) = (Ringer::default(), Instant::now());
        ringer.step(true, t0);
        assert_eq!(ringer.step(false, t0 + s(3.0)), Some(Sound::Stop));
        assert_eq!(ringer.wakeup(), None);
        assert_eq!(ringer.step(true, t0 + s(4.0)), Some(Sound::Start), "a new alarm starts over");
    }
}
