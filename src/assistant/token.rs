//! The secret token hooks must present: 32 random bytes as hex, compared in constant time.

use std::fmt::Write;

const BYTES: usize = 32;

pub(super) fn fresh() -> Option<String> {
    let bytes = entropy()?;
    let mut token = String::with_capacity(BYTES * 2);
    for byte in bytes {
        let _ = write!(token, "{byte:02x}");
    }
    Some(token)
}

pub(super) fn same(given: &str, expected: &str) -> bool {
    given.len() == expected.len()
        && given.bytes().zip(expected.bytes()).fold(0u8, |diff, (left, right)| diff | (left ^ right)) == 0
}

#[cfg(windows)]
fn entropy() -> Option<[u8; BYTES]> {
    use windows::Win32::System::Com::CoCreateGuid;

    let first = unsafe { CoCreateGuid() }.ok()?.to_u128();
    let second = unsafe { CoCreateGuid() }.ok()?.to_u128();
    let mut bytes = [0; BYTES];
    bytes[..16].copy_from_slice(&first.to_le_bytes());
    bytes[16..].copy_from_slice(&second.to_le_bytes());
    Some(bytes)
}

#[cfg(unix)]
fn entropy() -> Option<[u8; BYTES]> {
    use std::io::Read;

    let mut bytes = [0; BYTES];
    std::fs::File::open("/dev/urandom").ok()?.read_exact(&mut bytes).ok()?;
    Some(bytes)
}

#[cfg(not(any(windows, unix)))]
fn entropy() -> Option<[u8; BYTES]> {
    None
}
