//! The macOS app bundle: unzipping it and reading its version.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const BUNDLE: &str = "notchbase.app";
pub const BUNDLE_BINARY: &str = "Contents/MacOS/notchbase";
const DITTO: &str = "/usr/bin/ditto";
const APP_EXTENSION: &str = "app";
const INFO: &str = "Contents/Info.plist";
const VERSION_KEY: &str = "<key>CFBundleShortVersionString</key>";
const OPEN: &str = "<string>";
const CLOSE: &str = "</string>";

// ditto rather than unzip, so the bundle's symlinks and attributes survive.
pub fn unzip(download: &Path, into: &Path) -> Command {
    let mut command = Command::new(DITTO);
    command.args(["-x", "-k"]).arg(download).arg(into);
    command
}

pub fn app_in(dir: &Path) -> io::Result<Option<PathBuf>> {
    let mut entries = std::fs::read_dir(dir)?.flatten().map(|entry| entry.path());
    Ok(entries.find(|path| path.extension().is_some_and(|extension| extension == APP_EXTENSION)))
}

pub fn bundle_version(app: &Path) -> Option<String> {
    plist_version(&std::fs::read_to_string(app.join(INFO)).ok()?)
}

// Good enough for our own Info.plist, no need for a real plist parser.
fn plist_version(plist: &str) -> Option<String> {
    let after = plist.split(VERSION_KEY).nth(1)?;
    let start = after.find(OPEN)? + OPEN.len();
    let end = after[start..].find(CLOSE)?;
    Some(after[start..start + end].trim().to_owned())
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::*;

    const PLIST: &str = "<?xml version=\"1.0\"?>\n<plist version=\"1.0\">\n<dict>\n\
        <key>CFBundleName</key>\n<string>notchbase</string>\n\
        <key>CFBundleShortVersionString</key>\n<string> 0.16.2 </string>\n\
        <key>CFBundleVersion</key>\n<string>42</string>\n</dict>\n</plist>\n";

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("notchbase-release-bundle-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn unzip_uses_ditto() {
        let command = unzip(Path::new("/tmp/notchbase.zip"), Path::new("/tmp/unpacked"));
        assert_eq!(command.get_program(), OsStr::new("/usr/bin/ditto"));
        let args: Vec<&OsStr> = command.get_args().collect();
        assert_eq!(args, ["-x", "-k", "/tmp/notchbase.zip", "/tmp/unpacked"].map(OsStr::new));
    }

    #[test]
    fn finds_the_dot_app() {
        let dir = scratch("found");
        std::fs::write(dir.join("readme.txt"), "").unwrap();
        std::fs::create_dir_all(dir.join("__MACOSX")).unwrap();
        std::fs::create_dir_all(dir.join("notchbase.app").join("Contents")).unwrap();
        assert_eq!(app_in(&dir).unwrap(), Some(dir.join("notchbase.app")));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn no_app_or_missing_folder() {
        let dir = scratch("empty");
        std::fs::write(dir.join("notchbase.application"), "").unwrap();
        assert_eq!(app_in(&dir).unwrap(), None);
        assert!(app_in(&dir.join("missing")).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reads_short_version_from_plist() {
        assert_eq!(plist_version(PLIST).as_deref(), Some("0.16.2"));
        assert_eq!(plist_version("<key>CFBundleVersion</key><string>42</string>"), None);
        assert_eq!(plist_version("<key>CFBundleShortVersionString</key>"), None);
        assert_eq!(plist_version("<key>CFBundleShortVersionString</key><string>1.0"), None);
        assert_eq!(plist_version("<key>CFBundleShortVersionString</key><string></string>").as_deref(), Some(""));
    }

    #[test]
    fn bundle_version_of_installed_app() {
        let app = scratch("versioned").join(BUNDLE);
        std::fs::create_dir_all(app.join("Contents")).unwrap();
        std::fs::write(app.join(INFO), PLIST).unwrap();
        assert_eq!(bundle_version(&app).as_deref(), Some("0.16.2"));
        assert_eq!(bundle_version(&app.with_file_name("other.app")), None);
        std::fs::remove_dir_all(app.parent().unwrap()).unwrap();
    }

    #[test]
    fn binary_path_inside_bundle() {
        let binary = Path::new("/Applications").join(BUNDLE).join(BUNDLE_BINARY);
        assert_eq!(binary, Path::new("/Applications/notchbase.app/Contents/MacOS/notchbase"));
    }
}
