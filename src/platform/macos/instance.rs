//! One running copy per data folder, held with an exclusive flock on notchbase.lock.

use std::fs::{File, OpenOptions};
use std::os::fd::AsRawFd;
use std::path::Path;

const LOCK_FILE: &str = "notchbase.lock";

pub struct HeldInstance {
    _file: Option<File>,
}

pub fn hold_instance(dir: &Path) -> Option<HeldInstance> {
    let Some(file) = open(dir) else {
        return Some(HeldInstance { _file: None });
    };
    let locked = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0;
    if locked {
        return Some(HeldInstance { _file: Some(file) });
    }
    let busy = std::io::Error::last_os_error().raw_os_error() == Some(libc::EWOULDBLOCK);
    (!busy).then_some(HeldInstance { _file: None })
}

fn open(dir: &Path) -> Option<File> {
    std::fs::create_dir_all(dir).ok()?;
    OpenOptions::new().read(true).write(true).create(true).truncate(false).open(dir.join(LOCK_FILE)).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("notchbase-instance-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn second_claim_fails_until_dropped() {
        let dir = scratch("twice");
        let first = hold_instance(&dir).expect("the first claim");
        assert!(dir.join(LOCK_FILE).exists());
        assert!(hold_instance(&dir).is_none(), "a second claim must fail");
        drop(first);
        assert!(hold_instance(&dir).is_some(), "a claim after the drop must succeed");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn different_folders_do_not_collide() {
        let (one, two) = (scratch("one"), scratch("two"));
        let first = hold_instance(&one).expect("the first folder");
        assert!(hold_instance(&two).is_some());
        drop(first);
        let _ = std::fs::remove_dir_all(&one);
        let _ = std::fs::remove_dir_all(&two);
    }
}
