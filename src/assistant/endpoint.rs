//! assistant.json, where hooks find the server: its port and token, written privately through a temporary file.

use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::token;

const FILE_NAME: &str = "assistant.json";
const MAX_BYTES: u64 = 4096;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct Endpoint {
    pub(super) port: u16,
    pub(super) token: String,
}

pub(super) fn read(dir: &Path) -> Option<Endpoint> {
    let file = dir.join(FILE_NAME);
    if std::fs::metadata(&file).ok()?.len() > MAX_BYTES {
        return None;
    }
    serde_json::from_slice(&std::fs::read(file).ok()?).ok()
}

pub(super) fn publish(dir: &Path, endpoint: &Endpoint) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let bytes = serde_json::to_vec(endpoint).map_err(io::Error::other)?;
    let temp = dir.join(format!("{FILE_NAME}.{}.tmp", std::process::id()));
    let _ = std::fs::remove_file(&temp);
    let written = write_private(&temp, &bytes).and_then(|()| std::fs::rename(&temp, dir.join(FILE_NAME)));
    if written.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    written
}

/// Removes assistant.json, but only while it still holds our token, so a newer copy's file stays.
pub(super) fn retract(dir: &Path, token: &str) {
    if read(dir).is_some_and(|endpoint| token::same(&endpoint.token, token)) {
        let _ = std::fs::remove_file(dir.join(FILE_NAME));
    }
}

fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}
