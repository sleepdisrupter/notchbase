//! Free names for new files, `name`, `name 2`, `name 3` and so on, claimed without replacing anything.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const MAX_NAME_TRIES: u32 = 10_000;

pub(super) fn numbered(base: &str, n: u32) -> String {
    if n == 1 { base.to_owned() } else { format!("{base} {n}") }
}

pub(super) fn claim_free_name<T>(
    dir: &Path,
    name: impl Fn(u32) -> String,
    create_without_replacing: impl Fn(&Path) -> io::Result<T>,
) -> io::Result<(PathBuf, T)> {
    for n in 1..=MAX_NAME_TRIES {
        let path = dir.join(name(n));
        if taken(&path) {
            continue;
        }
        match create_without_replacing(&path) {
            Ok(created) => return Ok((path, created)),
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists || taken(&path) => {}
            Err(err) => return Err(err),
        }
    }
    Err(io::Error::new(io::ErrorKind::AlreadyExists, "no free name"))
}

pub(super) fn write_new_file(dir: &Path, stem: &str, ext: &str, bytes: &[u8]) -> io::Result<PathBuf> {
    let (path, mut file) = claim_free_name(
        dir,
        |n| format!("{}.{ext}", numbered(stem, n)),
        |path| OpenOptions::new().write(true).create_new(true).open(path),
    )?;
    if let Err(err) = file.write_all(bytes).and_then(|()| file.flush()) {
        drop(file);
        let _ = fs::remove_file(&path);
        return Err(err);
    }
    Ok(path)
}

pub(super) fn move_into_free_name(from: &Path, dir: &Path, stem: &str, ext: &str) -> io::Result<PathBuf> {
    let (path, ()) =
        claim_free_name(dir, |n| format!("{}.{ext}", numbered(stem, n)), |path| move_without_replacing(from, path))?;
    Ok(path)
}

fn move_without_replacing(from: &Path, to: &Path) -> io::Result<()> {
    match fs::hard_link(from, to) {
        Ok(()) => {
            let _ = fs::remove_file(from);
            Ok(())
        }
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => Err(err),
        Err(_) if taken(to) => Err(io::ErrorKind::AlreadyExists.into()),
        Err(_) => fs::rename(from, to),
    }
}

fn taken(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}
