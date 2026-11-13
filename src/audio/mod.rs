//! The sound mixer: per-app volume and mute, the output and input levels, and picking the default devices. Windows
//! works through audio sessions and macOS through Core Audio process taps.

mod activity;
mod driver;
mod grouping;
mod keys;
mod labels;
mod mixer;
mod per_kind;
mod sorting;

#[cfg(any(target_os = "macos", test))]
mod core_audio;

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

use std::sync::Arc;

pub use mixer::Mixer;

#[cfg(target_os = "macos")]
pub(crate) use self::core_audio::bundle_ids::parent_bundle_id;
#[cfg(target_os = "macos")]
pub(crate) use self::keys::{pid_in_key, pid_key};
#[cfg(target_os = "macos")]
pub use self::macos::{Listener, master, recording_apps};
#[cfg(target_os = "macos")]
pub(crate) use self::macos::{file_icon, load_label, responsible_pid, running_app};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DeviceKind {
    Output,
    Input,
}

pub type AppId = u32;
pub const MASTER: AppId = u32::MAX;
pub const INPUT: AppId = u32::MAX - 1;

#[derive(Clone)]
pub struct AudioApp {
    pub id: AppId,
    pub name: String,
    pub volume: f32,
    pub muted: bool,
    pub peak: f32,
    pub icon: Option<Arc<egui::ColorImage>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub kind: DeviceKind,
    pub is_default: bool,
    pub form: DeviceForm,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DeviceForm {
    Speakers,
    Headphones,
    Display,
    Microphone,
    Other,
}

impl DeviceKind {
    const ALL: [Self; 2] = [Self::Output, Self::Input];

    fn for_row(id: AppId) -> Option<Self> {
        match id {
            MASTER => Some(Self::Output),
            INPUT => Some(Self::Input),
            _ => None,
        }
    }

    fn row_id(self) -> AppId {
        match self {
            Self::Output => MASTER,
            Self::Input => INPUT,
        }
    }

    fn fallback_name(self) -> &'static str {
        match self {
            Self::Output => "Output",
            Self::Input => "Input",
        }
    }
}
