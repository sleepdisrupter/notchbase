use std::io;
use std::path::PathBuf;

use super::*;

const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
const AGENT: &str = "notchbase/0.16.2";

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("notchbase-release-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct BreaksOff;

impl Read for BreaksOff {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        Err(io::ErrorKind::ConnectionReset.into())
    }
}

fn release(sha256: &str) -> Release {
    Release { version: "1.0.0".into(), url: "https://x/notchbase.zip".into(), sha256: sha256.into(), size: None }
}

#[test]
fn save_hashes_and_reports_progress() {
    let dir = std::env::temp_dir().join(format!("notchbase-update-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let to = dir.join("file.bin");
    let mut seen = Vec::new();
    let digest = save(&b"abc"[..], &to, Some(3), &mut |p| seen.push(p)).unwrap();
    assert_eq!(digest, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    assert_eq!(std::fs::read(&to).unwrap(), b"abc");
    assert_eq!(seen, [1.0]);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn no_size_no_progress() {
    let scratch = Scratch::new("unsized");
    let mut seen = Vec::new();
    assert_eq!(save(&b"abc"[..], &scratch.path("file.bin"), None, &mut |p| seen.push(p)).unwrap(), ABC);
    assert!(seen.is_empty());
}

#[test]
fn progress_caps_at_one() {
    let scratch = Scratch::new("oversized");
    let mut seen = Vec::new();
    save(&b"abcdef"[..], &scratch.path("file.bin"), Some(3), &mut |p| seen.push(p)).unwrap();
    assert_eq!(seen, [1.0]);
}

#[test]
fn broken_download_is_interrupted() {
    let scratch = Scratch::new("broken");
    let body = (&b"ab"[..]).chain(BreaksOff);
    assert_eq!(save(body, &scratch.path("file.bin"), None, &mut |_| {}).unwrap_err(), INTERRUPTED);
}

#[test]
fn save_into_missing_folder_fails() {
    let scratch = Scratch::new("nowhere");
    assert!(save(&b"abc"[..], &scratch.path("missing/file.bin"), None, &mut |_| {}).is_err());
}

#[test]
fn checksum_ignores_case() {
    let scratch = Scratch::new("placed");
    let to = scratch.path("notchbase.zip");
    let listed = release(&format!("  {}  ", ABC.to_uppercase()));
    place(&b"abc"[..], &listed, &to, Some(3), |_| {}).unwrap();
    assert_eq!(std::fs::read(&to).unwrap(), b"abc");
    assert!(!scratch.path("notchbase.part").exists());
}

#[test]
fn mismatch_keeps_old_file() {
    let scratch = Scratch::new("mismatch");
    let to = scratch.path("notchbase.zip");
    std::fs::write(&to, b"old").unwrap();
    assert_eq!(place(&b"abd"[..], &release(ABC), &to, None, |_| {}).unwrap_err(), MISMATCH);
    assert_eq!(std::fs::read(&to).unwrap(), b"old");
    assert!(!scratch.path("notchbase.part").exists());
}

#[test]
fn interrupted_download_leaves_no_part() {
    let scratch = Scratch::new("partless");
    let to = scratch.path("notchbase.zip");
    let body = (&b"ab"[..]).chain(BreaksOff);
    assert_eq!(place(body, &release(ABC), &to, None, |_| {}).unwrap_err(), INTERRUPTED);
    assert!(!to.exists() && !scratch.path("notchbase.part").exists());
}

#[test]
fn listed_size_beats_header() {
    assert_eq!(expected_size(Some(10), Some(20)), Some(10));
    assert_eq!(expected_size(None, Some(20)), Some(20));
    assert_eq!(expected_size(None, None), None);
    assert_eq!(expected_size(Some(0), Some(20)), None);
    assert_eq!(expected_size(None, Some(0)), None);
}

const MANIFEST_TEXT: &str = r#"{
    "version": "0.24.0",
    "notes": "Updates now come from GitHub.",
    "builds": {
        "macos-aarch64": {
            "url": "https://github.com/sleepdisrupter/notchbase/releases/download/v0.24.0/notchbase-0.24.0-macos-aarch64.zip",
            "sha256": "ab12",
            "size": 42
        },
        "windows-x86_64": {
            "url": "https://github.com/sleepdisrupter/notchbase/releases/download/v0.24.0/notchbase-0.24.0-windows-x86_64.exe",
            "sha256": "cd34"
        }
    }
}"#;

fn manifest() -> Manifest {
    serde_json::from_str(MANIFEST_TEXT).expect("a manifest")
}

#[test]
fn reads_latest_json_from_the_newest_release() {
    let server = Server::new(REPO, AGENT);
    assert_eq!(server.manifest, "https://github.com/sleepdisrupter/notchbase/releases/latest/download/latest.json");
}

#[test]
fn a_fork_reads_its_own_releases() {
    let server = Server::new(" /someone/fork/ ", AGENT);
    assert_eq!(server.manifest, "https://github.com/someone/fork/releases/latest/download/latest.json");
}

#[test]
fn built_in_repo_defaults_to_notchbase() {
    let repo = built_in_repo();
    assert!(!repo.trim().is_empty());
    if option_env!("NOTCHBASE_RELEASES_REPO").is_none_or(|repo| repo.trim().is_empty()) {
        assert_eq!(repo, REPO);
    }
}

#[test]
fn manifest_gives_this_platforms_build() {
    let found = manifest().release_for("macos-aarch64");
    assert_eq!(
        found,
        [Release {
            version: "0.24.0".into(),
            url: "https://github.com/sleepdisrupter/notchbase/releases/download/v0.24.0/notchbase-0.24.0-macos-aarch64.zip"
                .into(),
            sha256: "ab12".into(),
            size: Some(42),
        }]
    );
    let windows = manifest().release_for("windows-x86_64");
    assert_eq!((windows[0].sha256.as_str(), windows[0].size), ("cd34", None));
}

#[test]
fn missing_platform_means_no_update() {
    assert!(manifest().release_for("macos-x86_64").is_empty());
    let empty: Manifest = serde_json::from_str(r#"{"version":"0.24.0"}"#).unwrap();
    assert!(empty.release_for("macos-aarch64").is_empty());
}

#[test]
fn same_or_older_version_is_not_an_update() {
    let found = manifest().release_for("macos-aarch64");
    assert!(crate::newest(found.clone(), crate::Version::parse("0.24.0").unwrap()).is_none());
    assert!(crate::newest(found.clone(), crate::Version::parse("0.25.1").unwrap()).is_none());
    assert!(crate::newest(found, crate::Version::parse("0.23.24").unwrap()).is_some());
}

#[test]
fn build_needs_a_url_and_checksum() {
    let no_sum = r#"{"version":"1.0.0","builds":{"macos-aarch64":{"url":"https://x/a.zip"}}}"#;
    assert!(serde_json::from_str::<Manifest>(no_sum).is_err());
    let no_url = r#"{"version":"1.0.0","builds":{"macos-aarch64":{"sha256":"ab"}}}"#;
    assert!(serde_json::from_str::<Manifest>(no_url).is_err());
    assert!(serde_json::from_str::<Manifest>(r#"{"builds":{}}"#).is_err());
}

#[test]
fn update_agent_config() {
    for timeout in [CHECK_TIMEOUT, DOWNLOAD_TIMEOUT] {
        let agent = agent(timeout, AGENT);
        let config = agent.config();
        assert!(config.https_only());
        assert!(config.http_status_as_error());
        assert!(config.max_redirects() > 0);
        assert_eq!(config.timeouts().global, Some(timeout));
        assert!(matches!(config.user_agent(), ureq::config::AutoHeaderValue::Provided(text) if text.as_str() == AGENT));
    }
}

#[test]
fn plain_http_server_is_refused() {
    let server = Server::at("http://example.invalid/latest.json", AGENT);
    assert_eq!(server.releases("macos-aarch64").unwrap_err(), "Couldn't check for updates");
}

#[test]
fn plain_http_download_is_refused() {
    let scratch = Scratch::new("plain");
    let to = scratch.path("notchbase.zip");
    let mut listed = release(ABC);
    listed.url = "http://example.invalid/notchbase.zip".into();
    assert_eq!(download(&listed, &to, AGENT, |_| {}).unwrap_err(), "Couldn't check for updates");
    assert!(!to.exists() && !scratch.path("notchbase.part").exists());
}

#[test]
fn describes_network_errors() {
    assert_eq!(describe(&ureq::Error::Timeout(ureq::Timeout::Global)), "The update server took too long");
    let offline = "Can't reach the update server";
    assert_eq!(describe(&ureq::Error::HostNotFound), offline);
    assert_eq!(describe(&ureq::Error::ConnectionFailed), offline);
    assert_eq!(describe(&ureq::Error::Io(io::ErrorKind::ConnectionReset.into())), offline);
    assert_eq!(describe(&ureq::Error::BodyExceedsLimit(LARGEST)), "The update is too big");
    assert_eq!(describe(&ureq::Error::StatusCode(503)), "The update server said no (HTTP 503)");
    assert_eq!(describe(&ureq::Error::TooManyRedirects), "Couldn't check for updates");
}
