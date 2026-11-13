//! Turns verbatim `\\?\` paths back into plain ones, which the shell can parse.

use std::ffi::OsString;
use std::path::{Component, Path, PathBuf, Prefix};

pub(crate) fn shell_parsable(path: &Path) -> PathBuf {
    let mut components = path.components();
    let Some(Component::Prefix(prefix)) = components.next() else {
        return path.to_path_buf();
    };
    let mut plain = match prefix.kind() {
        Prefix::VerbatimDisk(drive) => OsString::from(format!("{}:", char::from(drive))),
        Prefix::VerbatimUNC(server, share) => {
            let mut unc = OsString::from(r"\\");
            unc.push(server);
            unc.push(r"\");
            unc.push(share);
            unc
        }
        _ => return path.to_path_buf(),
    };
    plain.push(components.as_path());
    PathBuf::from(plain)
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::shell_parsable;

    #[test]
    fn verbatim_prefixes_become_plain_paths() {
        let parsable = |path: &str| shell_parsable(Path::new(path));
        assert_eq!(parsable(r"\\?\C:\Windows\explorer.exe"), PathBuf::from(r"C:\Windows\explorer.exe"));
        assert_eq!(parsable(r"\\?\UNC\server\share\dir\a.txt"), PathBuf::from(r"\\server\share\dir\a.txt"));
    }

    #[test]
    fn plain_paths_are_kept() {
        for path in [r"C:\Users\a.txt", r"\\server\share\a.txt", r"relative\a.txt"] {
            assert_eq!(shell_parsable(Path::new(path)), PathBuf::from(path));
        }
    }
}
