//! The driver decides when the backend works: only while a page asks for it, and for 1.5 seconds after. Windows runs it
//! on a thread of its own and macOS inline on the main thread.

mod cadence;
mod meters;
mod publisher;
mod snapshot;

#[cfg(not(target_os = "windows"))]
mod inline;
#[cfg(target_os = "windows")]
mod worker;

use std::time::Duration;

use super::{AppId, DeviceKind, imp};

#[cfg(not(target_os = "windows"))]
pub use inline::Inline as Driver;
pub use snapshot::Snapshot;
#[cfg(target_os = "windows")]
pub use worker::Worker as Driver;

const DEMAND_GRACE: Duration = Duration::from_millis(1500);

pub enum Command {
    Volume(AppId, f32),
    Muted(AppId, bool),
    DefaultDevice(DeviceKind, String),
}

impl Command {
    fn apply(self, engine: &mut imp::Backend) {
        match self {
            Self::Volume(id, volume) => engine.set_volume(id, volume),
            Self::Muted(id, muted) => engine.set_muted(id, muted),
            Self::DefaultDevice(kind, id) => engine.set_default_device(kind, &id),
        }
    }
}
