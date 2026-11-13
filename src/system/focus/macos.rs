//! On macOS, Focus is read from the Do Not Disturb database files, which the system can keep from us.

use std::io::ErrorKind;
use std::path::PathBuf;

use notchbase_sdk::protocol::Focus;

use super::{Reading, parse};

const ASSERTIONS: &str = "Library/DoNotDisturb/DB/Assertions.json";
const MODES: &str = "Library/DoNotDisturb/DB/ModeConfigurations.json";

pub(super) fn read() -> Reading {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else { return Reading::Seen(Focus::default()) };
    match std::fs::read_to_string(home.join(ASSERTIONS)) {
        Err(err) if err.kind() == ErrorKind::PermissionDenied => Reading::Unreadable,
        Err(_) => Reading::Seen(Focus::default()),
        Ok(assertions) => {
            let modes = std::fs::read_to_string(home.join(MODES)).unwrap_or_default();
            Reading::Seen(parse::focus(&assertions, &modes))
        }
    }
}
