//! Where the app gets installed on each platform.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

const APPLICATIONS: &str = "/Applications";
const USER_APPLICATIONS: &str = "Applications";
const PROGRAMS: &str = "Programs";
const FOLDER: &str = "notchbase";
const EXE: &str = "notchbase.exe";

pub fn mac_destination(bundle: &OsStr) -> Option<PathBuf> {
    applications_folder(Path::new(APPLICATIONS), std::env::var_os("HOME")).map(|folder| folder.join(bundle))
}

pub fn windows_destination() -> Option<PathBuf> {
    programs_folder(std::env::var_os("LOCALAPPDATA"))
}

// /Applications when we can write to it, otherwise ~/Applications.
fn applications_folder(system: &Path, home: Option<OsString>) -> Option<PathBuf> {
    if writable(system) {
        Some(system.to_owned())
    } else {
        home.map(|home| PathBuf::from(home).join(USER_APPLICATIONS))
    }
}

fn programs_folder(local: Option<OsString>) -> Option<PathBuf> {
    local.map(|local| PathBuf::from(local).join(PROGRAMS).join(FOLDER).join(EXE))
}

fn writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".notchbase-{}", std::process::id()));
    let wrote = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(probe);
    wrote
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("notchbase-release-destination-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn writable_probe_cleans_up() {
        let dir = scratch("writable");
        assert!(writable(&dir));
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0, "the probe is removed");
        assert!(!writable(&dir.join("missing")));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn prefers_system_applications() {
        let dir = scratch("system");
        assert_eq!(applications_folder(&dir, Some("/Users/someone".into())), Some(dir.clone()));
        assert_eq!(applications_folder(&dir, None), Some(dir.clone()));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn falls_back_to_home_applications() {
        let locked = std::env::temp_dir().join("notchbase-release-no-such-folder").join("Applications");
        let home = applications_folder(&locked, Some("/Users/someone".into()));
        assert_eq!(home, Some(PathBuf::from("/Users/someone").join("Applications")));
        assert_eq!(applications_folder(&locked, None), None, "no home folder, nowhere to go");
    }

    #[test]
    fn windows_uses_local_programs() {
        let local = Some(OsString::from(r"C:\Users\someone\AppData\Local"));
        let exe = programs_folder(local).expect("a destination");
        assert_eq!(exe, PathBuf::from(r"C:\Users\someone\AppData\Local").join("Programs").join("notchbase").join(EXE));
        assert_eq!(exe.file_name(), Some(OsStr::new("notchbase.exe")));
        assert_eq!(programs_folder(None), None);
    }

    #[test]
    fn mac_destination_ends_in_bundle() {
        if let Some(app) = mac_destination(OsStr::new("notchbase beta.app")) {
            assert_eq!(app.file_name(), Some(OsStr::new("notchbase beta.app")));
            assert!(app.parent().is_some_and(|folder| folder.ends_with("Applications")));
        }
    }
}
