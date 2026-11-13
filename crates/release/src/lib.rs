//! Versions, the release list and the install contract shared by the app and the installer.

pub mod install;
mod release;
mod server;
mod version;

pub use release::{Release, newest, platform};
pub use server::{Build, MANIFEST, Manifest, REPO, Server, built_in_repo, download};
pub use version::Version;
