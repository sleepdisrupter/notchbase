//! Networking shared by the app and the installer: HTTPS agents and hashed downloads.

mod agent;
mod copy;
mod hash;

pub use agent::{Mode, agent, config};
pub use copy::{CopyError, Limits, copy_hashed, fraction};
pub use hash::{file_sha256, hex, sha256_hex};
