//! A path's key: a hash of its names chained from the root, so a path can be looked up without being stored.

use std::path::{Component, Path};

const FILE_NAMES_IGNORE_CASE: bool = cfg!(any(windows, target_os = "macos"));
const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const PRIME: u64 = 0x0000_0100_0000_01b3;
const END_OF_NAME: u8 = 0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Key(pub u64);

pub const START: Key = Key(OFFSET_BASIS);

pub fn chain(parent: Key, name: &str) -> Key {
    let mixed = name.bytes().map(identity_byte).chain([END_OF_NAME]).fold(parent.0, mix);
    Key(mixed)
}

pub fn key_of(path: &Path) -> Key {
    path.components().fold(START, |key, component| match component {
        Component::RootDir => key,
        named => chain(key, &named.as_os_str().to_string_lossy()),
    })
}

fn identity_byte(byte: u8) -> u8 {
    if FILE_NAMES_IGNORE_CASE { byte.to_ascii_lowercase() } else { byte }
}

fn mix(state: u64, byte: u8) -> u64 {
    (state ^ u64::from(byte)).wrapping_mul(PRIME)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_chains_components() {
        assert_eq!(key_of(Path::new("/")), START);
        let work = chain(chain(START, "Volumes"), "Work");
        assert_eq!(key_of(Path::new("/Volumes/Work")), work);
        assert_eq!(key_of(Path::new("/Volumes/Work/notes.txt")), chain(work, "notes.txt"));
        assert_ne!(work, START);
    }

    #[test]
    fn root_and_name_chain_whole() {
        for root in ["/", "/Volumes/Work", "C:\\"] {
            let below = Path::new(root).join("Users").join("john");
            assert_eq!(key_of(&below), chain(chain(key_of(Path::new(root)), "Users"), "john"), "{root}");
        }
    }

    #[test]
    fn drive_letter_is_one_link() {
        let drive = key_of(Path::new("C:\\"));
        let expected = if cfg!(windows) { chain(START, "C:") } else { chain(START, "C:\\") };
        assert_eq!(drive, expected);
    }

    #[test]
    fn case_counts_where_kept() {
        let same = chain(START, "README.md") == chain(START, "readme.md");
        assert_eq!(same, FILE_NAMES_IGNORE_CASE);
        assert_eq!(key_of(Path::new("/Users/John")) == key_of(Path::new("/users/john")), FILE_NAMES_IGNORE_CASE);
    }

    #[test]
    fn accents_are_part_of_the_name() {
        assert_ne!(chain(START, "resume.pdf"), chain(START, "r\u{e9}sum\u{e9}.pdf"));
        assert_ne!(chain(START, "\u{c9}t\u{e9}"), chain(START, "\u{e9}t\u{e9}"), "only ASCII letters fold");
    }

    #[test]
    fn name_boundaries_matter() {
        assert_ne!(chain(chain(START, "ab"), "c"), chain(chain(START, "a"), "bc"));
        assert_ne!(chain(START, ""), START);
    }

    #[test]
    fn hash_is_fnv_1a() {
        assert_eq!(b"a".iter().fold(OFFSET_BASIS, |state, byte| mix(state, *byte)), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(b"foobar".iter().fold(OFFSET_BASIS, |state, byte| mix(state, *byte)), 0x8594_4171_f739_67e8);
    }
}
