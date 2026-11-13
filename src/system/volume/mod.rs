//! Volume, brightness and media keys. The notch can take the keys over to show its own indicator, and it watches the
//! output so a change made anywhere else shows too.

mod detector;
mod steps;
mod tap;
mod watch;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use self::macos as imp;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
use self::windows as imp;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod other;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use self::other as imp;

#[cfg(target_os = "macos")]
pub use imp::may_watch_keys;
pub use watch::Volume;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Level {
    pub volume: f32,
    pub muted: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Reading {
    pub device: String,
    pub level: Level,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Press {
    Up,
    Down,
    Mute,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyPress {
    pub press: Press,
    pub fine: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BrightnessPress {
    pub up: bool,
    pub fine: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaKey {
    PlayPause,
    Next,
    Previous,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Volume(KeyPress),
    Media(MediaKey),
    // Only a Mac catches the brightness keys.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    Brightness(BrightnessPress),
}
