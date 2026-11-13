//! Unzips next to the zip. It unpacks into a hidden folder first, so a broken zip leaves nothing
//! behind, and a zip holding one folder isn't nested in another.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const STAGING: &str = "unzipping";
const MAC_LEFTOVERS: &str = "__MACOSX"; // resource forks a Mac adds to the zips it makes
const COULDNT_READ: &str = "Couldn't read the zip";

pub fn is_zip(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
}

pub fn unzip(archive: &Path) -> Result<PathBuf, String> {
    let parent = archive.parent().ok_or(COULDNT_READ)?;
    let stem = archive.file_stem().ok_or(COULDNT_READ)?.to_string_lossy().into_owned();
    let staging = parent.join(format!(".{stem}.{STAGING}.{}", std::process::id()));
    fs::create_dir_all(&staging).map_err(|err| err.to_string())?;
    let unpacked = extract(archive, &staging).and_then(|()| settle(&staging, parent, &stem));
    let _ = fs::remove_dir_all(&staging);
    unpacked
}

fn extract(archive: &Path, into: &Path) -> Result<(), String> {
    let status =
        extractor(archive, into).stdout(Stdio::null()).stderr(Stdio::null()).status().map_err(|err| err.to_string())?;
    if status.success() { Ok(()) } else { Err(COULDNT_READ.to_owned()) }
}

#[cfg(target_os = "macos")]
fn extractor(archive: &Path, into: &Path) -> Command {
    let mut command = Command::new("/usr/bin/ditto");
    command.arg("-x").arg("-k").arg(archive).arg(into);
    command
}

#[cfg(windows)]
fn extractor(archive: &Path, into: &Path) -> Command {
    use std::os::windows::process::CommandExt as _;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut command = Command::new("tar");
    command.arg("-xf").arg(archive).arg("-C").arg(into).creation_flags(CREATE_NO_WINDOW);
    command
}

#[cfg(not(any(target_os = "macos", windows)))]
fn extractor(archive: &Path, into: &Path) -> Command {
    let mut command = Command::new("unzip");
    command.arg("-q").arg(archive).arg("-d").arg(into);
    command
}

fn settle(staging: &Path, parent: &Path, stem: &str) -> Result<PathBuf, String> {
    let _ = fs::remove_dir_all(staging.join(MAC_LEFTOVERS));
    let entries: Vec<PathBuf> =
        fs::read_dir(staging).map_err(|err| err.to_string())?.filter_map(|entry| Some(entry.ok()?.path())).collect();
    let (from, name) = match entries.as_slice() {
        [only] => (only.clone(), only.file_name().ok_or(COULDNT_READ)?.to_string_lossy().into_owned()),
        _ => (staging.to_path_buf(), stem.to_owned()),
    };
    let to = free_name(parent, &name);
    fs::rename(&from, &to).map_err(|err| err.to_string())?;
    Ok(to)
}

fn free_name(parent: &Path, name: &str) -> PathBuf {
    let wanted = parent.join(name);
    if !wanted.exists() {
        return wanted;
    }
    let (base, extension) = match name.rsplit_once('.') {
        Some((base, extension)) if !base.is_empty() => (base, format!(".{extension}")),
        _ => (name, String::new()),
    };
    (2..)
        .map(|n| parent.join(format!("{base} {n}{extension}")))
        .find(|candidate| !candidate.exists())
        .expect("some number is free")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("notchbase-unzip-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[cfg(target_os = "macos")]
    fn zip(dir: &Path, name: &str, files: &[&str]) -> PathBuf {
        let source = dir.join("source");
        for file in files {
            let path = source.join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, file).unwrap();
        }
        let archive = dir.join(name);
        let zipped = Command::new("zip").arg("-qr").arg(&archive).arg(".").current_dir(&source).status().unwrap();
        assert!(zipped.success());
        fs::remove_dir_all(source).unwrap();
        archive
    }

    #[test]
    fn only_zips_are_offered() {
        assert!(is_zip(Path::new("/Downloads/fonts.zip")) && is_zip(Path::new("/Downloads/FONTS.ZIP")));
        assert!(!is_zip(Path::new("/Downloads/movie.mp4")) && !is_zip(Path::new("/Downloads/zip")));
    }

    #[test]
    fn taken_name_gets_a_number() {
        let dir = scratch("names");
        assert_eq!(free_name(&dir, "fonts"), dir.join("fonts"));
        fs::create_dir(dir.join("fonts")).unwrap();
        fs::write(dir.join("notes.txt"), "").unwrap();
        assert_eq!(free_name(&dir, "fonts"), dir.join("fonts 2"));
        assert_eq!(free_name(&dir, "notes.txt"), dir.join("notes 2.txt"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn loose_files_get_a_folder() {
        let dir = scratch("loose");
        let archive = zip(&dir, "fonts.zip", &["a.ttf", "b.ttf"]);
        let folder = unzip(&archive).unwrap();
        assert_eq!(folder, dir.join("fonts"));
        assert_eq!(fs::read_to_string(folder.join("b.ttf")).unwrap(), "b.ttf");
        assert!(archive.exists(), "the zip itself stays");
        assert_eq!(unzip(&archive).unwrap(), dir.join("fonts 2"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn single_folder_is_not_nested() {
        let dir = scratch("single");
        let archive = zip(&dir, "project-main.zip", &["project/readme.md", "project/src/main.rs"]);
        let folder = unzip(&archive).unwrap();
        assert_eq!(folder, dir.join("project"));
        assert!(folder.join("src/main.rs").exists());
        let left: Vec<_> = fs::read_dir(&dir).unwrap().map(|entry| entry.unwrap().file_name()).collect();
        assert_eq!(left.len(), 2, "no staging folder is left behind: {left:?}");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn broken_zip_leaves_nothing() {
        let dir = scratch("broken");
        let archive = dir.join("broken.zip");
        fs::write(&archive, "not a zip").unwrap();
        assert!(unzip(&archive).is_err());
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        fs::remove_dir_all(dir).unwrap();
    }
}
