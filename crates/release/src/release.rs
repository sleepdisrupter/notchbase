//! One row of the release list, and picking the newest one.

use serde::Deserialize;

use crate::Version;

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Release {
    pub version: String,
    pub url: String,
    pub sha256: String,
    #[serde(default)]
    pub size: Option<u64>,
}

pub fn newest(releases: Vec<Release>, after: Version) -> Option<(Version, Release)> {
    releases
        .into_iter()
        .filter_map(|release| Some((Version::parse(&release.version)?, release)))
        .filter(|(version, _)| *version > after)
        .max_by_key(|(version, _)| *version)
}

pub fn platform() -> String {
    format!("{}-{}", std::env::consts::OS, native_arch())
}

// An Intel build running under Rosetta should update to the Apple Silicon build.
fn native_arch() -> &'static str {
    if cfg!(all(target_os = "macos", target_arch = "x86_64")) && under_rosetta() {
        "aarch64"
    } else {
        std::env::consts::ARCH
    }
}

fn under_rosetta() -> bool {
    std::process::Command::new("/usr/sbin/sysctl")
        .args(["-n", "sysctl.proc_translated"])
        .output()
        .is_ok_and(|output| output.stdout.trim_ascii() == b"1")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(version: &str) -> Release {
        Release { version: version.into(), url: format!("https://x/{version}"), sha256: String::new(), size: None }
    }

    fn version(text: &str) -> Version {
        Version::parse(text).expect("a version")
    }

    #[test]
    fn newest_release_wins() {
        let current = version("0.2.0");
        let releases = vec![release("0.1.9"), release("0.3.0"), release("0.2.1"), release("junk")];
        let (found_version, found) = newest(releases, current).expect("an update");
        assert_eq!(found_version, version("0.3.0"));
        assert_eq!(found.url, "https://x/0.3.0");
        assert!(newest(vec![release("0.2.0"), release("0.1.0")], current).is_none());
    }

    #[test]
    fn platform_names_os_and_arch() {
        let platform = platform();
        let known = ["windows-x86_64", "windows-aarch64", "macos-x86_64", "macos-aarch64"];
        assert!(platform.starts_with(std::env::consts::OS));
        assert!(!cfg!(any(windows, target_os = "macos")) || known.contains(&platform.as_str()), "{platform}");
    }

    #[test]
    fn no_releases_means_no_update() {
        assert_eq!(newest(Vec::new(), Version::default()), None);
    }

    #[test]
    fn skips_unreadable_versions() {
        let releases = vec![release("soon"), release("1.2.3.4"), release("")];
        assert_eq!(newest(releases, Version::default()), None);
    }

    #[test]
    fn release_versions_parse_like_versions() {
        let (found, release) = newest(vec![release("v1.0.0-beta")], Version::default()).expect("an update");
        assert_eq!(found, version("1.0.0"));
        assert_eq!(release.version, "v1.0.0-beta");
    }

    #[test]
    fn later_duplicate_wins() {
        let mut second = release("1.0.0");
        second.url = "https://x/second".into();
        let (_, found) = newest(vec![release("1.0.0"), second], Version::default()).expect("an update");
        assert_eq!(found.url, "https://x/second");
    }

    #[test]
    fn row_size_is_optional() {
        let text = r#"[{"version":"0.16.2","url":"https://x/a.zip","sha256":"ab","size":42},{"version":"0.16.1","url":"https://x/b.zip","sha256":"cd","notes":"ignored"}]"#;
        let rows: Vec<Release> = serde_json::from_str(text).unwrap();
        assert_eq!(rows[0].size, Some(42));
        assert_eq!(rows[1].size, None);
        assert_eq!((rows[1].version.as_str(), rows[1].sha256.as_str()), ("0.16.1", "cd"));
    }

    #[test]
    fn row_needs_a_checksum() {
        assert!(serde_json::from_str::<Release>(r#"{"version":"1.0.0","url":"https://x/a.zip"}"#).is_err());
    }
}
