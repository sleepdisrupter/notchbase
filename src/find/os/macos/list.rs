//! Lists a folder with getattrlistbulk, and stats a single path.

use std::cell::RefCell;
use std::fs::{File, Metadata, OpenOptions};
use std::io::{self, ErrorKind};
use std::os::fd::AsRawFd as _;
use std::os::macos::fs::MetadataExt as _;
use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::Path;

use crate::find::os::{Listed, Raw, raw};

use super::bulk::{ASKED, unpack};
use super::ffi::SF_DATALESS;

const BUFFER: usize = 128 * 1024;
const NO_OPTIONS: u64 = 0;
const DOT: u8 = b'.';

thread_local! {
    static BULK: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

pub fn list(dir: &Path, devices: [u64; 2], into: &mut Listed) -> io::Result<()> {
    into.names.clear();
    into.rows.clear();
    let folder = open(dir)?;
    BULK.with_borrow_mut(|buffer| {
        buffer.resize(BUFFER, 0);
        let mut refused = None;
        loop {
            let records = fill(&folder, buffer)?;
            if records == 0 {
                break;
            }
            refused = refused.or(unpack(buffer, records, devices, into));
        }
        refused.map_or(Ok(()), |code| Err(io::Error::from_raw_os_error(code)))
    })
}

pub fn stat(path: &Path) -> io::Result<Option<Raw>> {
    match std::fs::symlink_metadata(path) {
        Ok(found) => Ok(Some(row(path, &found))),
        Err(missing) if matches!(missing.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) => Ok(None),
        Err(refusal) => Err(refusal),
    }
}

fn open(dir: &Path) -> io::Result<File> {
    let asked = OpenOptions::new().read(true).custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW).open(dir);
    let folder = asked.map_err(|refusal| match refusal.raw_os_error() {
        Some(libc::ELOOP) => ErrorKind::NotADirectory.into(),
        _ => refusal,
    })?;
    if folder.metadata()?.st_flags() & SF_DATALESS != 0 {
        return Err(ErrorKind::Unsupported.into());
    }
    Ok(folder)
}

fn fill(folder: &File, buffer: &mut [u8]) -> io::Result<usize> {
    let mut asked = libc::attrlist {
        bitmapcount: libc::ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr: ASKED,
        volattr: 0,
        dirattr: 0,
        fileattr: 0,
        forkattr: 0,
    };
    loop {
        let records = unsafe {
            libc::getattrlistbulk(
                folder.as_raw_fd(),
                std::ptr::from_mut(&mut asked).cast(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                NO_OPTIONS,
            )
        };
        match usize::try_from(records) {
            Ok(records) => return Ok(records),
            Err(_) => {
                let failure = io::Error::last_os_error();
                if failure.kind() != ErrorKind::Interrupted {
                    return Err(failure);
                }
            }
        }
    }
}

fn row(path: &Path, found: &Metadata) -> Raw {
    let kind = found.file_type();
    let named_hidden = path.file_name().is_some_and(|name| name.as_bytes().first() == Some(&DOT));
    let mut bits = match (kind.is_symlink(), kind.is_dir()) {
        (true, _) => raw::LINK,
        (false, true) => raw::DIR,
        (false, false) => 0,
    };
    if named_hidden || found.st_flags() & libc::UF_HIDDEN != 0 {
        bits |= raw::HIDDEN;
    }
    if found.st_flags() & SF_DATALESS != 0 {
        bits |= raw::DATALESS | raw::CLOUD;
    }
    Raw { modified: seconds(found.st_mtime()), created: seconds(found.st_birthtime()), file_id: found.st_ino(), bits }
}

fn seconds(stamp: i64) -> u32 {
    u32::try_from(stamp.max(0)).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::os::unix::fs::{MetadataExt as _, symlink};

    use super::*;
    use crate::find::os::portable;
    use crate::find::tests::Scratch;

    const MANY: usize = 3_000;
    const NOWHERE: [u64; 2] = [1, 2];

    fn rows(listed: &Listed) -> BTreeMap<String, Raw> {
        let names = listed.names.split(|byte| *byte == 0).map(|name| String::from_utf8_lossy(name).into_owned());
        names.zip(listed.rows.iter().copied()).collect()
    }

    fn planted(name: &str) -> Scratch {
        let tree = Scratch::new(name);
        fs::create_dir_all(tree.file("Folder/inner")).expect("a folder");
        fs::create_dir(tree.file(".hidden")).expect("a hidden folder");
        fs::write(tree.file("Résumé final.pdf"), b"pdf").expect("a file");
        fs::write(tree.file(".env"), b"").expect("a hidden file");
        symlink(tree.file("Folder"), tree.file("to-folder")).expect("a link to a folder");
        symlink(tree.dir(), tree.file("Folder/loop")).expect("a link back up");
        symlink(tree.file("nowhere"), tree.file("dangling")).expect("a dangling link");
        tree
    }

    #[test]
    fn bulk_lister_matches_std() {
        let tree = planted("bulk-agrees");
        for index in 0..MANY {
            fs::write(tree.file(&format!("Folder/inner/file-{index:05}.txt")), b"").expect("one of many files");
        }
        for dir in [tree.dir().to_path_buf(), tree.file("Folder"), tree.file("Folder/inner")] {
            let (mut bulk, mut plain) = (Listed::default(), Listed::default());
            list(&dir, [0, 0], &mut bulk).expect("the bulk listing");
            portable::list(&dir, [0, 0], &mut plain).expect("the standard listing");
            let (bulk, plain) = (rows(&bulk), rows(&plain));
            assert_eq!(bulk.keys().collect::<Vec<_>>(), plain.keys().collect::<Vec<_>>());
            for (name, row) in &bulk {
                let found = fs::symlink_metadata(dir.join(name)).expect("an entry that was listed");
                assert_eq!((row.file_id, row.modified), (found.ino(), seconds(found.mtime())), "{name}");
                assert_eq!(Raw { file_id: 0, ..*row }, plain[name], "{name}");
                assert_eq!(stat(&dir.join(name)).expect("a stat"), Some(*row), "{name}");
            }
        }
        let mut inner = Listed::default();
        list(&tree.file("Folder/inner"), [0, 0], &mut inner).expect("a folder larger than one buffer");
        assert_eq!(inner.rows.len(), MANY);
    }

    #[test]
    fn links_marked_never_followed() {
        let tree = planted("bulk-kinds");
        let mut top = Listed::default();
        list(tree.dir(), [0, 0], &mut top).expect("the listing");
        let top = rows(&top);
        let bits = |name: &str| top[name].bits;
        assert_eq!((bits("Folder"), bits(".hidden"), bits(".env")), (raw::DIR, raw::DIR | raw::HIDDEN, raw::HIDDEN));
        assert_eq!((bits("to-folder"), bits("dangling"), bits("Résumé final.pdf")), (raw::LINK, raw::LINK, 0));
        let mut through = Listed { names: b"stale\0".to_vec(), rows: vec![Raw::default()] };
        let refused = list(&tree.file("to-folder"), [0, 0], &mut through).expect_err("a link is not a folder");
        assert_eq!(refused.kind(), ErrorKind::NotADirectory);
        assert_eq!(through, Listed::default(), "a failed listing still clears what the buffer held");
        assert_eq!(
            list(&tree.file("gone"), [0, 0], &mut through).expect_err("nothing there").kind(),
            ErrorKind::NotFound
        );
    }

    #[test]
    fn foreign_folder_marked_elsewhere() {
        let tree = planted("bulk-devices");
        let here = fs::metadata(tree.dir()).expect("the tree").dev();
        let mut listed = Listed::default();
        list(tree.dir(), NOWHERE, &mut listed).expect("the listing");
        let fenced = rows(&listed);
        assert_eq!(fenced["Folder"].bits, raw::DIR | raw::ELSEWHERE);
        assert_eq!((fenced[".env"].bits, fenced["to-folder"].bits), (raw::HIDDEN, raw::LINK));
        list(tree.dir(), [NOWHERE[0], here], &mut listed).expect("the listing");
        assert_eq!(rows(&listed)["Folder"].bits, raw::DIR, "the second device of the pair is as good as the first");
    }

    #[test]
    fn missing_stats_gone_link_itself() {
        let tree = planted("bulk-stat");
        assert_eq!(stat(&tree.file("nothing")).expect("a clean miss"), None);
        assert_eq!(stat(&tree.file(".env/below-a-file")).expect("a miss below a file"), None);
        let link = stat(&tree.file("to-folder")).expect("a stat").expect("the link");
        assert_eq!(link.bits, raw::LINK);
        let folder = stat(&tree.file(".hidden")).expect("a stat").expect("the folder");
        assert_eq!(folder.bits, raw::DIR | raw::HIDDEN);
        assert!(folder.modified > 0 && folder.created > 0 && folder.file_id > 0);
    }
}
