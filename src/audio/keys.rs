//! App keys and ids. An id is a hash of the app's key, kept clear of the ids the output and input rows use.

use std::fmt::Display;

use super::{AppId, INPUT};

const PID_PREFIX: &str = "pid:";

pub fn app_id(key: &str) -> AppId {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in key.to_lowercase().bytes() {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(0x0100_0193);
    }
    if hash >= INPUT { hash - 2 } else { hash }
}

pub fn pid_key(pid: impl Display) -> String {
    format!("{PID_PREFIX}{pid}")
}

pub fn pid_in_key(key: &str) -> Option<&str> {
    key.strip_prefix(PID_PREFIX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::MASTER;

    #[test]
    fn ids_stable_and_never_reserved() {
        let a = app_id(r"C:\Program Files\Google\Chrome\Application\chrome.exe");
        let b = app_id(r"c:\program files\google\chrome\application\CHROME.EXE");
        assert_eq!(a, b);
        assert_ne!(a, app_id(r"C:\Apps\spotify.exe"));
        assert_eq!(app_id(""), 0x811c_9dc5);
        for key in ["", "a", "com.spotify.client", "#system"] {
            assert_ne!(app_id(key), MASTER);
            assert_ne!(app_id(key), INPUT);
        }
    }

    #[test]
    fn pid_keys_round_trip() {
        assert_eq!(pid_key(42), "pid:42");
        assert_eq!(pid_in_key(&pid_key(42)), Some("42"));
        assert_eq!(pid_in_key("com.spotify.client"), None);
    }
}
