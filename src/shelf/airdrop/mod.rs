//! Sending Shelf files with the system's own AirDrop window. Only a Mac has AirDrop.

#[cfg(target_os = "macos")]
mod macos;

use std::path::PathBuf;

use crate::poll::Wake;

/// Whether this computer has AirDrop. Elsewhere the Shelf has no way to send.
pub const AVAILABLE: bool = cfg!(target_os = "macos");

const NOTHING: &str = "Nothing to AirDrop";
#[cfg(not(target_os = "macos"))]
const ONLY_ON_A_MAC: &str = "AirDrop is only on a Mac";

/// How an AirDrop that was started ended, as AppKit tells it. Only a Mac ever starts one.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub enum Ended {
    Sent,
    Cancelled,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finished {
    pub id: u64,
    pub ended: Ended,
}

/// Opens the AirDrop window with `paths`. Its outcome comes back from `take_finished`
/// under `id`, after `wake` has been called.
pub fn send(id: u64, paths: &[PathBuf], wake: Wake) -> Result<(), String> {
    if paths.is_empty() {
        return Err(NOTHING.into());
    }
    #[cfg(target_os = "macos")]
    {
        macos::send(id, paths, wake)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (id, wake);
        Err(ONLY_ON_A_MAC.into())
    }
}

/// The AirDrops that ended since the last call, oldest first. Main thread only.
pub fn take_finished() -> Vec<Finished> {
    #[cfg(target_os = "macos")]
    {
        macos::take_finished()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    #[test]
    fn nothing_selected_sends_nothing() {
        assert_eq!(send(1, &[], Arc::new(|| {})), Err(NOTHING.to_owned()));
        assert!(take_finished().is_empty());
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn without_a_mac_there_is_no_airdrop() {
        let paths = [PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")];
        assert_eq!(send(1, &paths, Arc::new(|| {})), Err(ONLY_ON_A_MAC.to_owned()));
    }
}
