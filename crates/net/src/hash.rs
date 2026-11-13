//! SHA-256 helpers. Digests are always lowercase hex.

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use sha2::{Digest, Sha256};

pub(crate) const CHUNK: usize = 64 * 1024;

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

pub fn file_sha256(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; CHUNK];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            return Ok(hex(&hasher.finalize()));
        }
        hasher.update(&buffer[..read]);
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    const EMPTY: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("notchbase-net-hash-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn file(&self, bytes: &[u8]) -> PathBuf {
            let path = self.0.join("file.bin");
            std::fs::write(&path, bytes).unwrap();
            path
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn hex_is_lowercase_and_padded() {
        assert_eq!(hex(&[0, 15, 255]), "000fff");
        assert_eq!(hex(&[0xAB, 0xCD]), "abcd");
    }

    #[test]
    fn hex_of_nothing_is_empty() {
        assert_eq!(hex(&[]), "");
    }

    #[test]
    fn hex_uses_two_digits_per_byte() {
        let all: Vec<u8> = (0..=255).collect();
        let text = hex(&all);
        assert_eq!(text.len(), 512);
        assert!(text.starts_with("000102") && text.ends_with("fdfeff"));
        assert!(text.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)));
    }

    #[test]
    fn sha256_test_vectors() {
        assert_eq!(sha256_hex(b""), EMPTY);
        assert_eq!(sha256_hex(b"abc"), ABC);
    }

    #[test]
    fn sha256_of_unicode_uses_utf8() {
        let text = "\u{00fc}ber \u{1f600}";
        assert_eq!(sha256_hex(text.as_bytes()), "d81f15c387d7b3bb814846a0fc9509d5db53fb5955443d77ddaa6895aad3d197");
    }

    #[test]
    fn file_hash_matches_bytes() {
        let scratch = Scratch::new("file");
        assert_eq!(file_sha256(&scratch.file(b"abc")).unwrap(), ABC);
    }

    #[test]
    fn empty_file_hash() {
        let scratch = Scratch::new("empty");
        assert_eq!(file_sha256(&scratch.file(b"")).unwrap(), EMPTY);
    }

    #[test]
    fn large_file_hash() {
        let scratch = Scratch::new("large");
        let bytes: Vec<u8> = (0..CHUNK * 2 + 17).map(|index| (index % 251) as u8).collect();
        assert_eq!(file_sha256(&scratch.file(&bytes)).unwrap(), sha256_hex(&bytes));
    }

    #[test]
    fn missing_file_is_an_error() {
        let scratch = Scratch::new("missing");
        let err = file_sha256(&scratch.0.join("nowhere.bin")).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn folder_is_an_error() {
        let scratch = Scratch::new("folder");
        assert!(file_sha256(&scratch.0).is_err());
    }
}
