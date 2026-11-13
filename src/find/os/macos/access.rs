//! Checks for Full Disk Access without a prompt, by trying to read a folder only that access opens.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

const HOME: &str = "HOME";
const WITNESSES: [&str; 2] = ["Library/DoNotDisturb/DB", "Library/Application Support/com.apple.TCC"];

pub fn full_access() -> bool {
    std::env::var_os(HOME).map(PathBuf::from).is_some_and(|home| reads_protected_folders(&home))
}

fn reads_protected_folders(home: &Path) -> bool {
    for witness in WITNESSES {
        match std::fs::read_dir(home.join(witness)) {
            Ok(_) => return true,
            Err(refusal) if refusal.kind() == ErrorKind::PermissionDenied => return false,
            Err(_) => {}
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use std::fs::{self, Permissions};
    use std::os::unix::fs::PermissionsExt as _;

    use super::*;
    use crate::find::tests::Scratch;

    const LOCKED: u32 = 0o000;
    const OPEN: u32 = 0o700;

    #[test]
    fn no_witness_is_limited() {
        let home = Scratch::new("access-bare");
        assert!(!reads_protected_folders(home.dir()));
    }

    #[test]
    fn first_witness_decides() {
        let home = Scratch::new("access-first");
        let (focus, privacy) = (home.file(WITNESSES[0]), home.file(WITNESSES[1]));
        fs::create_dir_all(&privacy).expect("the second witness");
        assert!(reads_protected_folders(home.dir()), "the second one opens when the first is missing");
        fs::create_dir_all(&focus).expect("the first witness");
        fs::set_permissions(&focus, Permissions::from_mode(LOCKED)).expect("a locked folder");
        let limited = !reads_protected_folders(home.dir());
        fs::set_permissions(&focus, Permissions::from_mode(OPEN)).expect("an unlocked folder");
        assert!(limited, "a refusal on the first witness is the answer, whatever the second says");
        assert!(reads_protected_folders(home.dir()));
    }
}
