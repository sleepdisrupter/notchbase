//! The mixer on Windows, through WASAPI audio sessions on the default output.

mod app_labels;
mod app_names;
mod backend;
mod com;
mod default_endpoint;
mod device_labels;
mod endpoints;
mod icon_location;
mod icons;
mod policy_config;
mod processes;
mod session;
mod sessions;

pub use backend::Backend;
