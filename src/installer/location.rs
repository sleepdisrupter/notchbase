//! Where the app is and where it goes. A new copy is swapped in with the old one set aside, and settings go along.

use std::io;
use std::path::{Path, PathBuf};

/// The copy being replaced is moved aside under this suffix, and the new copy deletes it once it starts.
const ASIDE: &str = ".old";
/// What sits beside the app and goes with it.
const SETTINGS: [&str; 2] = ["config.json", "tools"];

/// The app on disk: a bare executable, or a Mac `.app` bundle holding one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Binary(PathBuf),
    Bundle(PathBuf),
}

impl Target {
    pub fn current() -> io::Result<Self> {
        let exe = std::env::current_exe()?;
        Ok(bundle_of(&exe).map_or(Self::Binary(exe), Self::Bundle))
    }

    pub fn path(&self) -> &Path {
        match self {
            Self::Binary(path) | Self::Bundle(path) => path,
        }
    }

    /// The folder the executable is in, where `config.json` and `tools` sit beside it.
    pub fn exe_dir(&self) -> Option<PathBuf> {
        match self {
            Self::Binary(exe) => exe.parent().map(Path::to_path_buf),
            Self::Bundle(app) => Some(app.join("Contents").join("MacOS")),
        }
    }

    /// Whether two targets are the same place on disk, following links.
    pub fn same_place(&self, other: &Self) -> bool {
        let canonical = |path: &Path| path.canonicalize().unwrap_or_else(|_| path.to_owned());
        canonical(self.path()) == canonical(other.path())
    }
}

pub fn aside(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(ASIDE);
    path.with_file_name(name)
}

pub fn remove(path: &Path) {
    let _ = if path.is_dir() { std::fs::remove_dir_all(path) } else { std::fs::remove_file(path) };
}

/// Puts a new copy in place with `place`, moving the old one aside first and back again if that fails.
pub fn swap(current: &Path, place: impl FnOnce(&Path) -> io::Result<()>) -> io::Result<()> {
    let old = aside(current);
    remove(&old);
    let moved = current.exists();
    if moved {
        std::fs::rename(current, &old)?;
    }
    place(current).inspect_err(|_| {
        remove(current);
        if moved {
            let _ = std::fs::rename(&old, current);
        }
    })
}

/// Copies the settings from one copy to another that doesn't have them yet.
pub fn carry_settings(from: &Target, to: &Target) {
    if let (Some(from), Some(to)) = (from.exe_dir(), to.exe_dir()) {
        fill(&from, &to);
    }
}

/// Copies the settings into `keep` before an uninstall, replacing what an earlier one kept.
pub fn keep_settings(from: &Target, keep: &Path) -> io::Result<()> {
    let Some(from) = from.exe_dir() else { return Ok(()) };
    for name in SETTINGS {
        let (source, kept) = (from.join(name), keep.join(name));
        if source.exists() {
            std::fs::create_dir_all(keep)?;
            remove(&kept);
            copy_tree(&source, &kept)?;
        }
    }
    Ok(())
}

pub fn restore_settings(kept: &Path, to: &Target) {
    if let Some(to) = to.exe_dir() {
        fill(kept, &to);
    }
}

/// Copies each setting across, but never over one that's already there.
fn fill(from: &Path, to: &Path) {
    for name in SETTINGS {
        let (source, destination) = (from.join(name), to.join(name));
        if source.exists() && !destination.exists() {
            let _ = copy_tree(&source, &destination);
        }
    }
}

pub fn copy_tree(from: &Path, to: &Path) -> io::Result<()> {
    if !from.is_dir() {
        return std::fs::copy(from, to).map(drop);
    }
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        copy_tree(&entry.path(), &to.join(entry.file_name()))?;
    }
    Ok(())
}

/// The `.app` around an executable in `Contents/MacOS`, on a Mac.
fn bundle_of(exe: &Path) -> Option<PathBuf> {
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let app = contents.parent()?;
    let inside = macos.file_name()? == "MacOS" && contents.file_name()? == "Contents";
    (cfg!(target_os = "macos") && inside && app.extension()? == "app").then(|| app.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("notchbase-{name}-{}", std::process::id()));
        remove(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn old_copy_sits_beside_the_new_one() {
        let exe = Path::new("dir").join("notchbase.exe");
        assert_eq!(aside(&exe), Path::new("dir").join("notchbase.exe.old"));
    }

    #[test]
    fn only_a_mac_app_counts_as_a_bundle() {
        let inside = Path::new("/Applications/notchbase.app/Contents/MacOS/notchbase");
        assert_eq!(bundle_of(inside).is_some(), cfg!(target_os = "macos"));
        assert!(bundle_of(Path::new("/usr/local/bin/notchbase")).is_none());
    }

    #[test]
    fn failed_swap_puts_original_back() {
        let dir = scratch("swap");
        let exe = dir.join("app.exe");
        std::fs::write(&exe, b"old").unwrap();
        assert!(swap(&exe, |_| Err(io::Error::other("no"))).is_err());
        assert_eq!(std::fs::read(&exe).unwrap(), b"old");
        swap(&exe, |to| std::fs::write(to, b"new")).unwrap();
        assert_eq!(std::fs::read(&exe).unwrap(), b"new");
        assert_eq!(std::fs::read(aside(&exe)).unwrap(), b"old");
        remove(&dir);
    }

    #[test]
    fn swap_into_empty_place_just_places() {
        let dir = scratch("place");
        let exe = dir.join("app.exe");
        swap(&exe, |to| std::fs::write(to, b"new")).unwrap();
        assert_eq!(std::fs::read(&exe).unwrap(), b"new");
        assert!(!aside(&exe).exists());
        remove(&dir);
    }

    #[test]
    fn settings_follow_without_overwriting() {
        let dir = scratch("carry");
        let (old, new) = (dir.join("old"), dir.join("new"));
        std::fs::create_dir_all(old.join("tools")).unwrap();
        std::fs::create_dir_all(&new).unwrap();
        std::fs::write(old.join("config.json"), b"mine").unwrap();
        std::fs::write(old.join("tools/plugin.dll"), b"p").unwrap();
        std::fs::write(new.join("tools"), b"").unwrap();
        carry_settings(&Target::Binary(old.join("a.exe")), &Target::Binary(new.join("a.exe")));
        assert_eq!(std::fs::read(new.join("config.json")).unwrap(), b"mine");
        assert!(new.join("tools").is_file(), "left alone");
        assert_eq!(
            Target::Bundle(PathBuf::from("/x/n.app")).exe_dir(),
            Some(Path::new("/x/n.app").join("Contents").join("MacOS"))
        );
        remove(&dir);
    }

    #[test]
    fn uninstall_keeps_newest_settings() {
        let dir = scratch("keep");
        let (app, kept) = (dir.join("app"), dir.join("kept"));
        std::fs::create_dir_all(app.join("tools")).unwrap();
        std::fs::create_dir_all(kept.join("tools")).unwrap();
        std::fs::write(app.join("config.json"), b"new").unwrap();
        std::fs::write(app.join("tools/plugin.dll"), b"p").unwrap();
        std::fs::write(kept.join("config.json"), b"old").unwrap();
        std::fs::write(kept.join("tools/gone.dll"), b"g").unwrap();
        keep_settings(&Target::Binary(app.join("a.exe")), &kept).unwrap();
        assert_eq!(std::fs::read(kept.join("config.json")).unwrap(), b"new");
        assert_eq!(std::fs::read(kept.join("tools/plugin.dll")).unwrap(), b"p");
        assert!(!kept.join("tools/gone.dll").exists());
        remove(&dir);
    }

    #[test]
    fn nothing_to_keep_leaves_kept_alone() {
        let dir = scratch("keep-none");
        let (app, kept) = (dir.join("app"), dir.join("kept"));
        std::fs::create_dir_all(&app).unwrap();
        keep_settings(&Target::Binary(app.join("a.exe")), &kept).unwrap();
        assert!(!kept.exists());
        std::fs::create_dir_all(&kept).unwrap();
        std::fs::write(kept.join("config.json"), b"old").unwrap();
        keep_settings(&Target::Binary(app.join("a.exe")), &kept).unwrap();
        assert_eq!(std::fs::read(kept.join("config.json")).unwrap(), b"old");
        remove(&dir);
    }

    #[test]
    fn reinstall_restores_kept_settings() {
        let dir = scratch("restore");
        let (kept, fresh, used) = (dir.join("kept"), dir.join("fresh"), dir.join("used"));
        std::fs::create_dir_all(kept.join("tools")).unwrap();
        std::fs::create_dir_all(&fresh).unwrap();
        std::fs::create_dir_all(&used).unwrap();
        std::fs::write(kept.join("config.json"), b"mine").unwrap();
        std::fs::write(kept.join("tools/plugin.dll"), b"p").unwrap();
        std::fs::write(used.join("config.json"), b"theirs").unwrap();
        restore_settings(&kept, &Target::Binary(fresh.join("a.exe")));
        restore_settings(&kept, &Target::Binary(used.join("a.exe")));
        assert_eq!(std::fs::read(fresh.join("config.json")).unwrap(), b"mine");
        assert_eq!(std::fs::read(fresh.join("tools/plugin.dll")).unwrap(), b"p");
        assert_eq!(std::fs::read(used.join("config.json")).unwrap(), b"theirs");
        assert_eq!(std::fs::read(used.join("tools/plugin.dll")).unwrap(), b"p");
        assert!(kept.join("config.json").exists());
        remove(&dir);
    }

    #[test]
    fn folders_copy_with_everything_inside() {
        let dir = scratch("tree");
        std::fs::create_dir_all(dir.join("a/b")).unwrap();
        std::fs::write(dir.join("a/b/c.txt"), b"c").unwrap();
        copy_tree(&dir.join("a"), &dir.join("z")).unwrap();
        assert_eq!(std::fs::read(dir.join("z/b/c.txt")).unwrap(), b"c");
        remove(&dir);
    }
}
