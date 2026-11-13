//! Small watchers and switches for the operating system: power and sleep, Bluetooth, volume and brightness keys, Focus,
//! the calendar, downloads, the camera and mic, the lock screen, the menu bar, the summon key and permissions. Each one
//! has a macOS side, a Windows side and an `other` stub so the app still builds elsewhere.

pub(crate) mod awake;
pub(crate) mod bluetooth;
pub(crate) mod brightness;
pub(crate) mod calendar;
pub(crate) mod dock;
pub(crate) mod downloads;
pub(crate) mod focus;
pub(crate) mod menus;
pub(crate) mod permissions;
pub(crate) mod power;
pub(crate) mod sensors;
pub(crate) mod session;
pub(crate) mod summon;
pub(crate) mod volume;
