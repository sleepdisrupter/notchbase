//! Bluetooth devices coming and going, so the notch can show a headset connecting with its battery level.

mod kind;
mod tracker;
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

pub use kind::DeviceKind;
pub use watch::Bluetooth;

#[derive(Clone, Debug, PartialEq)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub kind: DeviceKind,
    pub battery: Option<u8>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    Connected(Device),
    Disconnected(Device),
    Updated(Device),
}
