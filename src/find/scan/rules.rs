//! The names and places the plan knows: exclusions from the config, packages, media libraries, backups, toolchains,
//! caches and the folders where builds cause storms of changes. Names compare the way the file system does.

use std::ffi::OsStr;
use std::path::Path;

use crate::stats::disk::plan::DEVELOPER;
use crate::stats::disk::scan::PACKAGES;

pub(super) const PROJECTS: &str = "Developer";
pub(super) const VERSIONED: &str = ".git";
pub(super) const CARGO_OUTPUT: &str = "target";
pub(super) const HOME_SYSTEM: &str = "Library";
pub(super) const SYSTEM_PLACES: [&str; 6] = ["/Library", "/opt", "/private", "/usr", "/cores", "/nix"];
pub(super) const SYSTEM_NAMES: [&str; 4] = ["AppData", "ProgramData", "Program Files", "Program Files (x86)"];
pub(super) const WASTE_PLACES: [&str; 1] = ["/private/var/folders"];
pub(super) const WASTE: [&str; 8] = [
    ".Trash",
    ".Trashes",
    ".cache",
    "__pycache__",
    ".Spotlight-V100",
    ".fseventsd",
    ".DocumentRevisions-V100",
    ".TemporaryItems",
];
pub(super) const WASTE_BELOW_SYSTEM: [&str; 8] =
    ["Caches", "Cache", "tmp", "temp", "Code Cache", "GPUCache", "INetCache", "CachedData"];
pub(super) const STORMY: [&str; 4] = ["node_modules", "DerivedData", "Caches", "Cache"];
pub(super) const MEDIA: [&str; 3] = ["photoslibrary", "musiclibrary", "tvlibrary"];
pub(super) const BACKUP_ENDINGS: [&str; 3] = ["previous", "inprogress", "interrupted"];
pub(super) const BACKUP_STORE: &str = "Backups.backupdb";

const EXTENSION_MARK: char = '.';
const HOME_MARK: char = '~';
const SEPARATORS: [char; 2] = ['/', '\\'];

pub(super) enum Exclusion {
    Below(std::path::PathBuf),
    Named(String),
}

pub(super) fn exclusion(entry: &str, home: Option<&Path>) -> Option<Exclusion> {
    let entry = entry.trim();
    if let Some(below_home) = entry.strip_prefix(HOME_MARK) {
        return home.map(|home| Exclusion::Below(home.join(below_home.trim_start_matches(SEPARATORS))));
    }
    match entry {
        "" => None,
        path if path.contains(SEPARATORS) => Some(Exclusion::Below(path.into())),
        name => Some(Exclusion::Named(name.to_owned())),
    }
}

pub(super) fn alike(left: &str, right: &str) -> bool {
    if cfg!(any(windows, target_os = "macos")) { left.eq_ignore_ascii_case(right) } else { left == right }
}

pub(super) fn listed(names: &[&str], name: &str) -> bool {
    names.iter().any(|known| alike(known, name))
}

pub(super) fn under(path: &Path, prefix: &Path) -> bool {
    let mut parts = path.components();
    prefix.components().all(|wanted| parts.next().is_some_and(|part| same_part(part.as_os_str(), wanted.as_os_str())))
}

pub(super) fn same(left: &Path, right: &Path) -> bool {
    left.as_os_str() == right.as_os_str() || (under(left, right) && under(right, left))
}

pub(super) fn placed(path: &Path, places: &[&str]) -> bool {
    places.iter().any(|place| same(path, Path::new(place)))
}

pub(super) fn ending(name: &str) -> Option<&str> {
    name.rsplit_once(EXTENSION_MARK).map(|(_, ending)| ending)
}

pub(super) fn package(name: &str) -> bool {
    ending(name).is_some_and(|ending| listed(&PACKAGES, ending))
}

pub(super) fn media(name: &str) -> bool {
    ending(name).is_some_and(|ending| listed(&MEDIA, ending))
}

pub(super) fn backup(name: &str) -> bool {
    alike(name, BACKUP_STORE) || ending(name).is_some_and(|ending| listed(&BACKUP_ENDINGS, ending))
}

pub(super) fn toolchain(name: &str) -> bool {
    listed(&DEVELOPER, name) && !alike(name, PROJECTS)
}

pub(super) fn stormy(name: &str) -> bool {
    let hidden_toolchain = name.starts_with(EXTENSION_MARK) && toolchain(name);
    alike(name, VERSIONED)
        || alike(name, CARGO_OUTPUT)
        || listed(&STORMY, name)
        || listed(&WASTE, name)
        || hidden_toolchain
}

pub(super) fn parts(path: &Path) -> impl Iterator<Item = std::borrow::Cow<'_, str>> {
    path.components().map(|part| part.as_os_str().to_string_lossy())
}

fn same_part(left: &OsStr, right: &OsStr) -> bool {
    left == right || left.to_str().zip(right.to_str()).is_some_and(|(left, right)| alike(left, right))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn under_matches_whole_parts() {
        let path = Path::new("/Users/me/Documents/Notes");
        assert!(under(path, Path::new("/Users/me")) && under(path, path) && under(path, Path::new("/")));
        assert!(!under(path, Path::new("/Users/me/Doc")), "half a name is not a folder");
        assert!(!under(Path::new("/Users/me"), path), "a parent is not under its child");
        assert!(same(Path::new("/Library"), Path::new("/Library/")) && !same(path, Path::new("/Users/me")));
        let drive = PathBuf::from("C:\\").join("Users").join("me");
        assert!(under(&drive, Path::new("C:\\")) && !under(&drive, Path::new("D:\\")));
    }

    #[test]
    fn names_compare_like_the_disk() {
        let folds = cfg!(any(windows, target_os = "macos"));
        assert_eq!(alike("Node_Modules", "node_modules"), folds);
        assert_eq!(under(Path::new("/users/ME/x"), Path::new("/Users/me")), folds);
        assert!(alike("résumé", "résumé") && !alike("resume", "résumé"), "accents are never folded");
    }

    #[test]
    fn endings_pick_out_packages() {
        assert!(
            package("Xcode.app") && package("Photos Library.photoslibrary") && !package("app") && !package("notes.txt")
        );
        assert!(media("Photos Library.photoslibrary") && media("Music.musiclibrary") && !media("Xcode.app"));
        assert!(backup("2026-10-06-101500.previous") && backup("Backups.backupdb") && !backup("previous"));
        assert!(toolchain(".cargo") && toolchain("go") && !toolchain("Developer") && !toolchain("Projects"));
        assert!(stormy("node_modules") && stormy(".git") && stormy("target") && stormy(".npm") && !stormy("src"));
        assert!(!stormy("go") && !stormy("Developer"), "only hidden toolchain folders are slowed down");
    }

    #[test]
    fn exclusion_is_path_or_name() {
        let home = Path::new("/Users/me");
        let below = |entry: &str| match exclusion(entry, Some(home)) {
            Some(Exclusion::Below(path)) => Some(path),
            _ => None,
        };
        assert_eq!(below("~/Private"), Some(PathBuf::from("/Users/me/Private")));
        assert_eq!(below("~"), Some(PathBuf::from("/Users/me")));
        assert_eq!(below("/Volumes/Work/tmp"), Some(PathBuf::from("/Volumes/Work/tmp")));
        assert_eq!(below("D:\\Scratch"), Some(PathBuf::from("D:\\Scratch")));
        assert!(matches!(exclusion(" build ", Some(home)), Some(Exclusion::Named(name)) if name == "build"));
        assert!(exclusion("", Some(home)).is_none() && exclusion("~/Private", None).is_none());
    }
}
