//! The mixer on macOS. Each app below full volume gets a Core Audio process tap that mutes it at the source, and its
//! audio is played back here at the chosen gain.

mod app_icon;
mod app_labels;
mod apps;
mod backend;
mod devices;
mod endpoint;
pub mod master;
mod permission;
mod processes;
mod property;
mod recording;
mod selectors;
mod streams;
mod symbols;
mod taps;

pub(crate) use app_icon::file_icon;
pub(crate) use app_labels::{load_label, running_app};
pub use backend::Backend;
pub(crate) use processes::responsible_pid;
pub use recording::recording_apps;
pub use taps::Listener;
