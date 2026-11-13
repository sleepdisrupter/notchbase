//! Listing and statting through the standard library, for systems without a faster way, and for the tests.

use std::ffi::OsStr;
use std::fs::Metadata;
use std::io::{self, ErrorKind};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use super::{Listed, Raw, raw};

const END_OF_NAME: u8 = 0;

pub fn list(dir: &Path, devices: [u64; 2], into: &mut Listed) -> io::Result<()> {
    into.names.clear();
    into.rows.clear();
    if std::fs::symlink_metadata(dir)?.file_type().is_symlink() {
        return Err(ErrorKind::NotADirectory.into());
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let Ok(found) = entry.metadata() else { continue };
        let name = entry.file_name();
        into.names.extend_from_slice(name.to_string_lossy().as_bytes());
        into.names.push(END_OF_NAME);
        into.rows.push(row(&name, &found, devices));
    }
    Ok(())
}

pub fn stat(path: &Path) -> io::Result<Option<Raw>> {
    match std::fs::symlink_metadata(path) {
        Ok(found) => Ok(Some(row(path.file_name().unwrap_or_default(), &found, [0, 0]))),
        Err(missing) if matches!(missing.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) => Ok(None),
        Err(refusal) => Err(refusal),
    }
}

fn row(name: &OsStr, found: &Metadata, devices: [u64; 2]) -> Raw {
    Raw {
        modified: seconds(found.modified()),
        created: seconds(found.created()),
        file_id: 0,
        bits: bits(name, found, devices),
    }
}

fn seconds(stamp: io::Result<SystemTime>) -> u32 {
    let since = stamp.ok().and_then(|stamp| stamp.duration_since(UNIX_EPOCH).ok());
    since.map_or(0, |since| u32::try_from(since.as_secs()).unwrap_or(u32::MAX))
}

#[cfg(unix)]
fn bits(name: &OsStr, found: &Metadata, devices: [u64; 2]) -> u8 {
    use std::os::unix::ffi::OsStrExt as _;
    use std::os::unix::fs::MetadataExt as _;

    const DOT: u8 = b'.';
    let kind = found.file_type();
    let mut bits = match (kind.is_symlink(), kind.is_dir()) {
        (true, _) => raw::LINK,
        (false, true) => raw::DIR,
        (false, false) => 0,
    };
    if name.as_bytes().first() == Some(&DOT) {
        bits |= raw::HIDDEN;
    }
    if bits & raw::DIR != 0 && devices != [0, 0] && !devices.contains(&found.dev()) {
        bits |= raw::ELSEWHERE;
    }
    bits
}

#[cfg(windows)]
fn bits(_name: &OsStr, found: &Metadata, _devices: [u64; 2]) -> u8 {
    use std::os::windows::fs::MetadataExt as _;

    const HIDDEN: u32 = 0x2;
    const SYSTEM: u32 = 0x4;
    const DIRECTORY: u32 = 0x10;
    const REPARSE_POINT: u32 = 0x400;
    const OFFLINE: u32 = 0x1000;
    const RECALL_ON_OPEN: u32 = 0x4_0000;
    const RECALL_ON_DATA_ACCESS: u32 = 0x40_0000;
    let attributes = found.file_attributes();
    let has = |attribute: u32| attributes & attribute != 0;
    let folder = has(DIRECTORY) && !has(REPARSE_POINT);
    let mut bits = match (has(REPARSE_POINT), folder) {
        (true, _) => raw::LINK,
        (false, true) => raw::DIR,
        (false, false) => 0,
    };
    if has(HIDDEN | SYSTEM) {
        bits |= raw::HIDDEN;
    }
    if has(OFFLINE | RECALL_ON_OPEN | RECALL_ON_DATA_ACCESS) {
        bits |= raw::CLOUD;
    }
    if folder && has(RECALL_ON_OPEN) {
        bits |= raw::DATALESS;
    }
    bits
}

#[cfg(not(any(unix, windows)))]
fn bits(_name: &OsStr, found: &Metadata, _devices: [u64; 2]) -> u8 {
    let kind = found.file_type();
    match (kind.is_symlink(), kind.is_dir()) {
        (true, _) => raw::LINK,
        (false, true) => raw::DIR,
        (false, false) => 0,
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::os::unix::fs::{MetadataExt as _, symlink};

    use super::*;
    use crate::find::tests::Scratch;

    const NOWHERE: [u64; 2] = [1, 2];

    fn planted(name: &str) -> Scratch {
        let tree = Scratch::new(name);
        fs::create_dir(tree.file("Folder")).expect("a folder");
        fs::create_dir(tree.file(".git")).expect("a hidden folder");
        fs::write(tree.file("notes.txt"), b"notes").expect("a file");
        symlink(tree.dir(), tree.file("loop")).expect("a link back to the tree");
        tree
    }

    fn bits_by_name(listed: &Listed) -> BTreeMap<String, u8> {
        let names =
            listed.names.split(|byte| *byte == END_OF_NAME).map(|name| String::from_utf8_lossy(name).into_owned());
        names.zip(listed.rows.iter().map(|row| row.bits)).collect()
    }

    #[test]
    fn lister_names_each_entry_once() {
        let tree = planted("portable-list");
        let mut listed = Listed { names: b"stale\0".to_vec(), rows: vec![Raw::default()] };
        list(tree.dir(), [0, 0], &mut listed).expect("the listing");
        let expected = [(".git", raw::DIR | raw::HIDDEN), ("Folder", raw::DIR), ("loop", raw::LINK), ("notes.txt", 0)];
        assert_eq!(bits_by_name(&listed), expected.map(|(name, bits)| (name.to_owned(), bits)).into());
        assert_eq!(listed.names.iter().filter(|byte| **byte == END_OF_NAME).count(), listed.rows.len());
        let written = fs::metadata(tree.file("notes.txt")).expect("the file").mtime();
        assert!(listed.rows.iter().all(|row| row.modified > 0 && row.created > 0 && row.file_id == 0));
        assert!(listed.rows.iter().any(|row| i64::from(row.modified) == written));
    }

    #[test]
    fn other_device_marked_link_unopened() {
        let tree = planted("portable-devices");
        let here = fs::metadata(tree.dir()).expect("the tree").dev();
        let mut listed = Listed::default();
        list(tree.dir(), NOWHERE, &mut listed).expect("the listing");
        let fenced = bits_by_name(&listed);
        assert_eq!((fenced["Folder"], fenced["loop"], fenced["notes.txt"]), (raw::DIR | raw::ELSEWHERE, raw::LINK, 0));
        list(tree.dir(), [here, here], &mut listed).expect("the listing");
        assert_eq!(bits_by_name(&listed)["Folder"], raw::DIR);
        let refused = list(&tree.file("loop"), [0, 0], &mut listed).expect_err("a link is not a folder");
        assert_eq!((refused.kind(), listed), (ErrorKind::NotADirectory, Listed::default()));
    }

    #[test]
    fn missing_stats_gone_found_as_row() {
        let tree = planted("portable-stat");
        assert_eq!(stat(&tree.file("nothing")).expect("a clean miss"), None);
        assert_eq!(stat(&tree.file("notes.txt/below")).expect("a miss below a file"), None);
        let hidden = stat(&tree.file(".git")).expect("a stat").expect("the folder");
        assert_eq!(hidden.bits, raw::DIR | raw::HIDDEN);
        assert_eq!(stat(&tree.file("loop")).expect("a stat").map(|row| row.bits), Some(raw::LINK));
        assert!(hidden.modified > 0);
    }

    #[test]
    fn early_times_count_as_unknown() {
        assert_eq!(seconds(Ok(UNIX_EPOCH - std::time::Duration::from_secs(5))), 0);
        assert_eq!(seconds(Err(ErrorKind::Unsupported.into())), 0);
        assert_eq!(seconds(Ok(UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000))), 1_700_000_000);
    }
}
