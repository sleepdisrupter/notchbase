//! Reads the login from the Keychain through /usr/bin/security, trying each name Claude Code may have saved it under.

use std::path::Path;
use std::process::{Command, Stdio};

use notchbase_net::sha256_hex;

use super::{Lookup, custom_config_dir, default_config_dir};

const SERVICE: &str = "Claude Code-credentials";
const SECURITY: &str = "/usr/bin/security";
const ITEM_NOT_FOUND: i32 = 44;
const SUFFIX_HEX_DIGITS: usize = 8;

pub(super) fn services() -> Vec<String> {
    let custom = custom_config_dir().map(|dir| suffixed_service(&dir));
    let default = default_config_dir().map(|dir| suffixed_service(&dir));
    let mut services: Vec<String> = custom.into_iter().chain([SERVICE.to_owned()]).collect();
    services.extend(default.filter(|service| !services.contains(service)));
    services
}

/// The item name for a config folder: the base name plus the first 8 hex digits of the folder path's SHA-256.
fn suffixed_service(config_dir: &Path) -> String {
    let hash = sha256_hex(config_dir.to_string_lossy().as_bytes());
    format!("{SERVICE}-{}", &hash[..SUFFIX_HEX_DIGITS])
}

pub(super) fn lookup(service: &str) -> Lookup {
    let output = Command::new(SECURITY)
        .args(["find-generic-password", "-s", service, "-w"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output();
    match output {
        Ok(out) if out.status.success() => decode_output(&out.stdout).map_or(Lookup::Missing, Lookup::Json),
        Ok(out) if out.status.code() == Some(ITEM_NOT_FOUND) => Lookup::Missing,
        Ok(_) => Lookup::Denied,
        Err(_) => Lookup::Missing,
    }
}

fn decode_output(raw: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(raw).ok()?.trim();
    if text.starts_with('{') {
        return Some(text.to_owned());
    }
    if text.is_empty() || text.len() % 2 != 0 || !text.bytes().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let bytes = (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).ok())
        .collect::<Option<Vec<u8>>>()?;
    String::from_utf8(bytes).ok().map(|s| s.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_decoding() {
        assert_eq!(decode_output(b"{\"a\":1}\n").as_deref(), Some("{\"a\":1}"));
        assert_eq!(decode_output(b"7b2261223a317d\n").as_deref(), Some("{\"a\":1}"));
        assert_eq!(decode_output(b"\n"), None);
        assert_eq!(decode_output(b"xyz"), None);
        assert_eq!(decode_output(b"7b2"), None);
        assert_eq!(decode_output(b"ff"), None);
    }

    #[test]
    fn legacy_service_is_listed_once() {
        let services = services();
        assert_eq!(services.iter().filter(|s| *s == SERVICE).count(), 1);
        assert_eq!(suffixed_service(Path::new("/Users/test/.claude")), "Claude Code-credentials-462977e4");
    }
}
