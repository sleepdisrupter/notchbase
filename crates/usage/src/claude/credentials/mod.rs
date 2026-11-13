//! The Claude Code login's OAuth token, from the macOS Keychain or .credentials.json. Its debug form is redacted.

mod keychain;

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::Value;

use super::json::number;

const FILE_NAME: &str = ".credentials.json";
const MAX_FILE_BYTES: u64 = 1024 * 1024;

#[derive(PartialEq)]
pub(super) struct Credentials {
    access_token: String,
    expires_at: Option<SystemTime>,
    from_keychain: bool,
}

impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credentials")
            .field("access_token", &"<redacted>")
            .field("expires_at", &self.expires_at)
            .field("from_keychain", &self.from_keychain)
            .finish()
    }
}

impl Credentials {
    pub(super) fn access_token(&self) -> &str {
        &self.access_token
    }

    pub(super) fn is_expired(&self, now: SystemTime) -> bool {
        self.expires_at.is_some_and(|at| at <= now)
    }

    /// Only a Keychain token is kept between refreshes, so the Keychain isn't asked every time.
    pub(super) fn into_cached(self) -> Option<Self> {
        self.from_keychain.then_some(self)
    }

    fn parse(json: &str, from_keychain: bool) -> Option<Self> {
        let root: Value = serde_json::from_str(json.trim()).ok()?;
        let oauth = root.get("claudeAiOauth")?;
        let token = oauth.get("accessToken")?.as_str()?.trim();
        if token.is_empty() {
            return None;
        }
        let expires_at = oauth
            .get("expiresAt")
            .and_then(number)
            .filter(|ms| *ms >= 0.0)
            .and_then(|ms| UNIX_EPOCH.checked_add(Duration::from_millis(ms as u64)));
        Some(Self { access_token: token.to_owned(), expires_at, from_keychain })
    }
}

pub(super) enum Found {
    Valid(Credentials),
    Expired,
    Missing,
    Denied,
}

enum Lookup {
    Json(String),
    Missing,
    Denied,
}

enum Source {
    Keychain(String),
    File(PathBuf),
}

impl Source {
    fn is_keychain(&self) -> bool {
        matches!(self, Source::Keychain(_))
    }

    fn lookup(&self) -> Lookup {
        match self {
            Source::Keychain(service) => keychain::lookup(service),
            Source::File(path) => read_small_file(path).map_or(Lookup::Missing, Lookup::Json),
        }
    }
}

pub(super) fn load(now: SystemTime) -> Found {
    let (mut expired, mut denied) = (false, false);
    for source in sources() {
        if denied && source.is_keychain() {
            continue;
        }
        let json = match source.lookup() {
            Lookup::Json(json) => json,
            Lookup::Missing => continue,
            Lookup::Denied => {
                denied = true;
                continue;
            }
        };
        match Credentials::parse(&json, source.is_keychain()) {
            Some(creds) if !creds.is_expired(now) => return Found::Valid(creds),
            Some(_) => expired = true,
            None => {}
        }
    }
    if denied {
        Found::Denied
    } else if expired {
        Found::Expired
    } else {
        Found::Missing
    }
}

fn sources() -> Vec<Source> {
    let services = if cfg!(target_os = "macos") { keychain::services() } else { Vec::new() };
    let file = config_dir().map(|dir| Source::File(dir.join(FILE_NAME)));
    services.into_iter().map(Source::Keychain).chain(file).collect()
}

fn read_small_file(path: &Path) -> Option<String> {
    if std::fs::metadata(path).ok()?.len() > MAX_FILE_BYTES {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

fn custom_config_dir() -> Option<PathBuf> {
    std::env::var_os("CLAUDE_CONFIG_DIR").filter(|dir| !dir.to_string_lossy().trim().is_empty()).map(PathBuf::from)
}

fn default_config_dir() -> Option<PathBuf> {
    let home = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(home).filter(|dir| !dir.is_empty()).map(|dir| PathBuf::from(dir).join(".claude"))
}

fn config_dir() -> Option<PathBuf> {
    custom_config_dir().or_else(default_config_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(secs: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(secs)
    }

    #[test]
    fn parse_and_expiry() {
        let json = r#"{
            "claudeAiOauth": {
                "accessToken": "sk-ant-oat01-FIXTURE",
                "refreshToken": "sk-ant-ort01-FIXTURE",
                "expiresAt": 1790000000000,
                "scopes": ["user:inference", "user:profile"],
                "subscriptionType": "max",
                "rateLimitTier": "default_claude_max_20x"
            },
            "mcpOAuth": {}
        }"#;
        let creds = Credentials::parse(json, false).unwrap();
        assert_eq!(creds.access_token(), "sk-ant-oat01-FIXTURE");
        assert_eq!(creds.expires_at, Some(at(1_790_000_000)));
        assert!(!creds.is_expired(at(1_789_999_999)));
        assert!(creds.is_expired(at(1_790_000_000)));
        assert!(!format!("{creds:?}").contains("FIXTURE"), "Debug must not print the token");

        let no_expiry = Credentials::parse(r#"{"claudeAiOauth": {"accessToken": "t"}}"#, false).unwrap();
        assert!(!no_expiry.is_expired(SystemTime::now()));
        let blank = r#"{"claudeAiOauth": {"accessToken": "", "refreshToken": "", "expiresAt": 0}}"#;
        assert_eq!(Credentials::parse(blank, false), None);
        let zero = Credentials::parse(r#"{"claudeAiOauth": {"accessToken": "t", "expiresAt": 0}}"#, false).unwrap();
        assert!(zero.is_expired(at(1)));
        assert_eq!(Credentials::parse(r#"{"mcpOAuth": {}}"#, false), None);
        assert_eq!(Credentials::parse("not json", false), None);
        assert_eq!(Credentials::parse(r#"{"claudeAiOauth": {"accessToken": 5}}"#, false), None);
    }

    #[test]
    fn only_keychain_reads_are_cached() {
        let json = r#"{"claudeAiOauth": {"accessToken": "t"}}"#;
        assert!(Credentials::parse(json, true).unwrap().into_cached().is_some());
        assert!(Credentials::parse(json, false).unwrap().into_cached().is_none());
    }

    #[test]
    fn oversized_credentials_are_ignored() {
        let dir = std::env::temp_dir().join(format!("notchbase-credentials-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (small, large) = (dir.join("small.json"), dir.join("large.json"));
        std::fs::write(&small, "{}").unwrap();
        std::fs::write(&large, vec![b' '; MAX_FILE_BYTES as usize + 1]).unwrap();
        assert_eq!(read_small_file(&small).as_deref(), Some("{}"));
        assert_eq!(read_small_file(&large), None);
        assert_eq!(read_small_file(&dir.join("missing.json")), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
