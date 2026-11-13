//! Where notchbase keeps its things: the data folder, config.json and the tools folders, each with an override in
//! the environment. An app bundle keeps config.json in the data folder, since writing into it breaks its signature.

use std::path::{Path, PathBuf};

use super::imp::{default_data_dir, default_local_data_dir};

const DATA_DIR_OVERRIDE: &str = "NOTCHBASE_DATA_DIR";
const CONFIG_OVERRIDE: &str = "NOTCHBASE_CONFIG";
const TOOLS_DIR_OVERRIDE: &str = "NOTCHBASE_TOOLS_DIR";
const CONFIG_FILE: &str = "config.json";
const TOOLS_DIR: &str = "tools";
const BUNDLE_EXE_DIR: &str = "MacOS";
const BUNDLE_CONTENTS: &str = "Contents";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnownDir {
    Home,
    Pictures,
    Videos,
    Desktop,
    Downloads,
    Documents,
}

pub fn data_dir() -> Option<PathBuf> {
    std::env::var_os(DATA_DIR_OVERRIDE).map(PathBuf::from).or_else(default_data_dir)
}

pub fn local_data_dir() -> Option<PathBuf> {
    std::env::var_os(DATA_DIR_OVERRIDE).map(PathBuf::from).or_else(default_local_data_dir)
}

pub fn install_dir() -> Option<PathBuf> {
    std::env::current_exe().ok()?.parent().map(PathBuf::from)
}

pub fn config_file() -> Option<PathBuf> {
    std::env::var_os(CONFIG_OVERRIDE).map(PathBuf::from).or_else(|| config_file_in(&install_dir()?))
}

pub fn config_file_in(dir: &Path) -> Option<PathBuf> {
    let beside = dir.join(CONFIG_FILE);
    if !in_bundle(dir) {
        return Some(beside);
    }
    let file = data_dir()?.join(CONFIG_FILE);
    adopt(&beside, &file);
    Some(file)
}

fn in_bundle(dir: &Path) -> bool {
    let named = |dir: Option<&Path>, name: &str| dir.and_then(Path::file_name).is_some_and(|found| found == name);
    cfg!(target_os = "macos") && named(Some(dir), BUNDLE_EXE_DIR) && named(dir.parent(), BUNDLE_CONTENTS)
}

fn adopt(old: &Path, file: &Path) {
    let moved = !file.exists()
        && old.is_file()
        && file.parent().is_some_and(|folder| std::fs::create_dir_all(folder).is_ok())
        && std::fs::copy(old, file).is_ok();
    if moved {
        let _ = std::fs::remove_file(old);
    }
}

pub fn tools_dir() -> Option<PathBuf> {
    std::env::var_os(TOOLS_DIR_OVERRIDE).map(PathBuf::from).or_else(|| data_dir().map(|dir| dir.join(TOOLS_DIR)))
}

pub fn tools_dirs() -> Vec<PathBuf> {
    if let Some(dir) = std::env::var_os(TOOLS_DIR_OVERRIDE) {
        return vec![PathBuf::from(dir)];
    }
    let mut dirs: Vec<PathBuf> = install_dir().map(|dir| dir.join(TOOLS_DIR)).into_iter().chain(tools_dir()).collect();
    dirs.dedup();
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("notchbase-paths-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn local_data_differs_only_on_windows() {
        let together = std::env::var_os(DATA_DIR_OVERRIDE).is_some() || !cfg!(windows);
        if together {
            assert_eq!(local_data_dir(), data_dir());
        } else {
            assert_ne!(local_data_dir(), data_dir());
        }
    }

    #[test]
    fn only_bundles_move_config_out() {
        let inside = Path::new("/Applications/notchbase.app/Contents/MacOS");
        assert_eq!(in_bundle(inside), cfg!(target_os = "macos"));
        assert!(!in_bundle(Path::new("/work/notchbase/target/debug")));
        assert!(!in_bundle(Path::new("/opt/MacOS")));
        let beside = Path::new("/work/notchbase/target/debug");
        assert_eq!(config_file_in(beside), Some(beside.join(CONFIG_FILE)));
    }

    #[test]
    fn bundle_settings_move_out_once() {
        let dir = scratch("adopt");
        let (old, file) = (dir.join("bundle.json"), dir.join("data").join(CONFIG_FILE));
        std::fs::write(&old, b"mine").unwrap();
        adopt(&old, &file);
        assert_eq!(std::fs::read(&file).unwrap(), b"mine");
        assert!(!old.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn moved_settings_are_never_overwritten() {
        let dir = scratch("keep");
        let (old, file) = (dir.join("bundle.json"), dir.join(CONFIG_FILE));
        std::fs::write(&old, b"stale").unwrap();
        std::fs::write(&file, b"current").unwrap();
        adopt(&old, &file);
        assert_eq!(std::fs::read(&file).unwrap(), b"current");
        assert!(old.exists());
        adopt(&dir.join("missing.json"), &dir.join("never").join(CONFIG_FILE));
        assert!(!dir.join("never").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
