//! The file frame index.bin and usage.bin share: a header, a table of tagged blocks packed with lz4, and a SHA-256
//! seal. A new file is written beside the old one and renamed over it, and a damaged one is removed.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use lz4_flex::block::{compress, decompress_into};
use sha2::{Digest, Sha256};

pub type Tag = [u8; 4];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub kind: u8,
    pub blocks: Vec<(Tag, Vec<u8>)>,
}

pub const INDEX: u8 = 1;
pub const USAGE: u8 = 2;
pub const MAX_BLOCK: u64 = 1 << 30;

const MAGIC: [u8; 8] = *b"NBFIND\r\n";
const VERSION: u32 = 1;
const THIS_OS: u8 = if cfg!(target_os = "macos") {
    1
} else if cfg!(windows) {
    2
} else {
    3
};
const HEADER: usize = 32;
const ENTRY: usize = 20;
const DIGEST: usize = 32;
const VERSION_AT: usize = 8;
const KIND_AT: usize = 12;
const OS_AT: usize = 13;
const COUNT_AT: usize = 14;
const LENGTH_AT: usize = 16;
const UNFINISHED: &str = ".tmp";

pub fn write(file: &Path, frame: &Frame) -> io::Result<()> {
    let bytes = encode(frame)?;
    if let Some(folder) = file.parent() {
        fs::create_dir_all(folder)?;
    }
    let unfinished = unfinished(file);
    let mut staged = fs::File::create(&unfinished)?;
    staged.write_all(&bytes)?;
    staged.sync_all()?;
    keep_private(&unfinished);
    fs::rename(&unfinished, file)
}

pub fn read(file: &Path, kind: u8) -> Option<Frame> {
    let bytes = fs::read(file).ok()?;
    let frame = decode(&bytes, kind);
    if frame.is_none() {
        let _ = fs::remove_file(file);
    }
    frame
}

fn unfinished(file: &Path) -> PathBuf {
    let mut beside = file.as_os_str().to_owned();
    beside.push(UNFINISHED);
    PathBuf::from(beside)
}

#[cfg(unix)]
fn keep_private(file: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(file, fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn keep_private(_file: &Path) {}

fn encode(frame: &Frame) -> io::Result<Vec<u8>> {
    let too_big = || io::Error::new(io::ErrorKind::InvalidInput, "The snapshot is too large to write");
    let count = u16::try_from(frame.blocks.len()).map_err(|_| too_big())?;
    if frame.blocks.iter().any(|(_, raw)| raw.len() as u64 > MAX_BLOCK) {
        return Err(too_big());
    }
    let packed: Vec<Vec<u8>> = frame.blocks.iter().map(|(_, raw)| compress(raw)).collect();
    let body: usize = packed.iter().map(Vec::len).sum();
    let length = HEADER + ENTRY * frame.blocks.len() + body + DIGEST;
    let mut bytes = Vec::with_capacity(length);
    bytes.extend(MAGIC);
    bytes.extend(VERSION.to_le_bytes());
    bytes.extend([frame.kind, THIS_OS]);
    bytes.extend(count.to_le_bytes());
    bytes.extend((length as u64).to_le_bytes());
    bytes.resize(HEADER, 0);
    for ((tag, raw), packed) in frame.blocks.iter().zip(&packed) {
        bytes.extend(tag);
        bytes.extend((raw.len() as u64).to_le_bytes());
        bytes.extend((packed.len() as u64).to_le_bytes());
    }
    packed.iter().for_each(|block| bytes.extend(block));
    let digest = Sha256::digest(&bytes);
    bytes.extend_from_slice(&digest);
    Ok(bytes)
}

fn decode(bytes: &[u8], kind: u8) -> Option<Frame> {
    let sealed = bytes.len().checked_sub(DIGEST).filter(|sealed| *sealed >= HEADER)?;
    let (content, digest) = bytes.split_at(sealed);
    let length = u64::from_le_bytes(content[LENGTH_AT..LENGTH_AT + 8].try_into().ok()?);
    let version = u32::from_le_bytes(content[VERSION_AT..KIND_AT].try_into().ok()?);
    let ours = content[..VERSION_AT] == MAGIC && version == VERSION && content[OS_AT] == THIS_OS;
    let whole = length == bytes.len() as u64 && Sha256::digest(content).as_slice() == digest;
    if !ours || content[KIND_AT] != kind || !whole {
        return None;
    }
    let count = usize::from(u16::from_le_bytes([content[COUNT_AT], content[COUNT_AT + 1]]));
    let directory = content.get(HEADER..HEADER + ENTRY * count)?;
    let mut rest = &content[HEADER + ENTRY * count..];
    let mut blocks = Vec::with_capacity(count);
    for entry in directory.as_chunks::<ENTRY>().0 {
        let tag: Tag = entry[..4].try_into().ok()?;
        let raw_len = u64::from_le_bytes(entry[4..12].try_into().ok()?);
        let packed_len = usize::try_from(u64::from_le_bytes(entry[12..ENTRY].try_into().ok()?)).ok()?;
        let packed = rest.get(..packed_len)?;
        rest = &rest[packed_len..];
        blocks.push((tag, unpack(packed, raw_len)?));
    }
    rest.is_empty().then_some(Frame { kind, blocks })
}

fn unpack(packed: &[u8], raw_len: u64) -> Option<Vec<u8>> {
    if raw_len > MAX_BLOCK {
        return None;
    }
    let mut raw = vec![0; raw_len as usize];
    let written = decompress_into(packed, &mut raw).ok()?;
    (written == raw.len()).then_some(raw)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::find::tests::Scratch;

    fn sample() -> Frame {
        let names = b"\0notchbase\0cargo.toml\0readme.md\0".repeat(400);
        Frame { kind: INDEX, blocks: vec![(*b"NAME", names), (*b"FLAG", vec![1, 0, 1]), (*b"NONE", Vec::new())] }
    }

    fn resealed(mut bytes: Vec<u8>, change: impl FnOnce(&mut [u8])) -> Vec<u8> {
        bytes.truncate(bytes.len() - DIGEST);
        change(&mut bytes);
        let digest = Sha256::digest(&bytes);
        bytes.extend_from_slice(&digest);
        bytes
    }

    fn refused(scratch: &Scratch, bytes: &[u8]) -> bool {
        let file = scratch.file(super::super::INDEX_FILE);
        fs::write(&file, bytes).unwrap();
        read(&file, INDEX).is_none() && !file.exists()
    }

    #[test]
    fn frame_round_trips_and_shrinks() {
        let scratch = Scratch::new("frame-round-trip");
        let file = scratch.file("nested/index.bin");
        assert!(read(&file, INDEX).is_none(), "a missing file is simply absent");
        write(&file, &sample()).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600, "for its owner only");
        }
        let back = read(&file, INDEX).expect("the file we just wrote");
        assert_eq!((back.kind, &back.blocks), (INDEX, &sample().blocks));
        let on_disk = fs::metadata(&file).unwrap().len() as usize;
        assert!(on_disk < sample().blocks[0].1.len() / 4, "{on_disk} bytes for repetitive names");
        assert!(!unfinished(&file).exists(), "nothing is left behind");
    }

    #[test]
    fn header_layout_is_stable() {
        let bytes = encode(&sample()).unwrap();
        assert_eq!(&bytes[..8], b"NBFIND\r\n");
        assert_eq!(bytes[8..14], [1, 0, 0, 0, INDEX, THIS_OS]);
        assert_eq!(bytes[14..16], 3u16.to_le_bytes());
        assert_eq!(bytes[16..24], (bytes.len() as u64).to_le_bytes());
        assert_eq!(bytes[24..HEADER], [0; 8]);
        assert_eq!(&bytes[HEADER..HEADER + 4], b"NAME");
    }

    #[test]
    fn damaged_file_is_removed() {
        let scratch = Scratch::new("frame-damaged");
        let bytes = encode(&sample()).unwrap();
        assert!(refused(&scratch, &bytes[..bytes.len() - 1]));
        assert!(refused(&scratch, &bytes[..HEADER + 3]));
        assert!(refused(&scratch, &[]));
        for at in [0, KIND_AT, HEADER + 5, bytes.len() / 2, bytes.len() - 1] {
            let mut flipped = bytes.clone();
            flipped[at] ^= 0x10;
            assert!(refused(&scratch, &flipped), "a flipped bit at {at}");
        }
        assert!(refused(&scratch, &[bytes.as_slice(), &[0]].concat()), "one byte too many");
    }

    #[test]
    fn foreign_frame_is_refused() {
        let scratch = Scratch::new("frame-foreign");
        let bytes = encode(&sample()).unwrap();
        assert!(refused(&scratch, &encode(&Frame { kind: USAGE, ..sample() }).unwrap()), "the other file");
        assert!(decode(&resealed(bytes.clone(), |_| {}), INDEX).is_some(), "resealing alone changes nothing");
        assert!(refused(&scratch, &resealed(bytes.clone(), |bytes| bytes[VERSION_AT] = 2)));
        assert!(refused(&scratch, &resealed(bytes.clone(), |bytes| bytes[0] = b'X')));
        assert!(refused(&scratch, &resealed(bytes.clone(), |bytes| bytes[OS_AT] = THIS_OS % 3 + 1)));
        assert!(refused(&scratch, &resealed(bytes, |bytes| bytes[LENGTH_AT] ^= 1)));
    }

    #[test]
    fn oversized_block_is_refused() {
        let scratch = Scratch::new("frame-claims");
        let bytes = encode(&sample()).unwrap();
        let raw_len_at = HEADER + 4;
        let claim = |claimed: u64| {
            resealed(bytes.clone(), |bytes| bytes[raw_len_at..raw_len_at + 8].copy_from_slice(&claimed.to_le_bytes()))
        };
        let honest = sample().blocks[0].1.len() as u64;
        assert!(decode(&claim(honest), INDEX).is_some());
        assert!(refused(&scratch, &claim(MAX_BLOCK + 1)));
        assert!(refused(&scratch, &claim(u64::MAX)));
        assert!(refused(&scratch, &claim(honest + 1)));
        assert!(refused(&scratch, &claim(honest - 1)));
        let packed_len_at = HEADER + 12;
        let spilling = resealed(bytes.clone(), |bytes| bytes[packed_len_at..packed_len_at + 8].fill(0xff));
        assert!(refused(&scratch, &spilling));
        let short_directory = resealed(bytes.clone(), |bytes| bytes[COUNT_AT] = 2);
        assert!(refused(&scratch, &short_directory), "bytes no block accounts for");
        assert!(refused(&scratch, &resealed(bytes, |bytes| bytes[COUNT_AT + 1] = 0xff)));
        let crowded = Frame { kind: INDEX, blocks: vec![(*b"NAME", Vec::new()); usize::from(u16::MAX) + 1] };
        assert_eq!(encode(&crowded).unwrap_err().kind(), io::ErrorKind::InvalidInput, "never written at all");
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_file_is_kept() {
        let scratch = Scratch::new("frame-unreadable");
        let file = scratch.file("usage.bin");
        std::os::unix::fs::symlink(&file, &file).unwrap();
        assert!(read(&file, USAGE).is_none());
        assert!(fs::symlink_metadata(&file).is_ok(), "only damaged contents are removed");
    }

    #[test]
    fn interrupted_write_keeps_old_file() {
        let scratch = Scratch::new("frame-interrupted");
        let file = scratch.file("index.bin");
        write(&file, &sample()).unwrap();
        fs::write(unfinished(&file), b"half a snapsh").unwrap();
        assert_eq!(read(&file, INDEX).map(|frame| frame.blocks), Some(sample().blocks));
        let newer = Frame { kind: INDEX, blocks: vec![(*b"FLAG", vec![9])] };
        write(&file, &newer).unwrap();
        assert_eq!(read(&file, INDEX).map(|frame| frame.blocks), Some(newer.blocks));
        assert!(!unfinished(&file).exists());
    }
}
