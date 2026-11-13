//! Turns the index into the blocks of index.bin and back. Parents are stored as steps from the row before and the
//! created time as an age before the modified time, which both pack far smaller.

use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, AtomicU32};

use crate::find::model::base::{Base, rank};
use crate::find::model::columns::Columns;
use crate::find::model::exotic::Exotics;

use super::frame::{Frame, INDEX, Tag};
use super::{Meta, RootMark, check};

pub(super) const HEAD: Tag = *b"HEAD";
pub(super) const PARENTS: Tag = *b"PRNT";
pub(super) const NAMES: Tag = *b"NAME";
pub(super) const CAPITALS: Tag = *b"CASE";
pub(super) const MODIFIED: Tag = *b"MTIM";
pub(super) const CREATED_BEFORE: Tag = *b"CAGE";
pub(super) const FLAGS: Tag = *b"FLAG";
pub(super) const EXOTIC: Tag = *b"XOTC";

const STAMP: usize = size_of::<u32>();
const BITS_PER_BYTE: usize = 8;
const WORD: usize = size_of::<u64>();

pub(super) fn encode(base: &Base, meta: &Meta) -> io::Result<Frame> {
    let columns = &base.columns;
    let mut below = 0;
    let steps =
        columns.parent.iter().map(|parent| rank(*parent).wrapping_sub(std::mem::replace(&mut below, rank(*parent))));
    let rows = 0..columns.len();
    let mut capitals: Vec<u8> = columns.upper.iter().flat_map(|word| word.to_le_bytes()).collect();
    capitals.truncate(columns.arena.len().div_ceil(BITS_PER_BYTE));
    let blocks = vec![
        (HEAD, head(columns, meta)?),
        (PARENTS, stamps(steps)),
        (NAMES, columns.arena.clone()),
        (CAPITALS, capitals),
        (MODIFIED, stamps(rows.clone().map(|row| columns.modified(row)))),
        (CREATED_BEFORE, stamps(rows.clone().map(|row| columns.modified(row).wrapping_sub(columns.created[row])))),
        (FLAGS, rows.map(|row| columns.flags(row)).collect()),
        (EXOTIC, exotic(&columns.exotic)?),
    ];
    Ok(Frame { kind: INDEX, blocks })
}

pub(super) fn decode(frame: Frame) -> Option<(Base, Meta)> {
    let mut blocks = frame.blocks;
    let mut block = |tag: Tag| blocks.iter_mut().find(|(found, _)| *found == tag).map(|(_, raw)| std::mem::take(raw));
    let (entries, name_bytes, meta) = read_head(&block(HEAD)?)?;
    let arena = block(NAMES).filter(|arena| arena.len() == name_bytes)?;
    let name_at = check::name_offsets(&arena, entries)?;
    let mut above = 0u32;
    let climb = |step: u32| {
        above = above.wrapping_add(step);
        above.wrapping_sub(1)
    };
    let parent: Vec<u32> = read_stamps(&block(PARENTS)?, entries)?.map(climb).collect();
    let modified: Vec<u32> = read_stamps(&block(MODIFIED)?, entries)?.collect();
    let ages = block(CREATED_BEFORE)?;
    let created =
        read_stamps(&ages, entries)?.zip(&modified).map(|(age, modified)| modified.wrapping_sub(age)).collect();
    let flags = block(FLAGS).filter(|flags| flags.len() == entries)?;
    let capitals = block(CAPITALS).filter(|bits| bits.len() == name_bytes.div_ceil(BITS_PER_BYTE))?;
    let widen = |bytes: &[u8]| {
        let mut word = [0; WORD];
        word[..bytes.len()].copy_from_slice(bytes);
        u64::from_le_bytes(word)
    };
    let columns = Columns {
        parent,
        name_at,
        modified: modified.into_iter().map(AtomicU32::new).collect(),
        created,
        flags: flags.into_iter().map(AtomicU8::new).collect(),
        arena,
        upper: capitals.chunks(WORD).map(widen).collect(),
        exotic: read_exotic(&block(EXOTIC)?)?,
    };
    let base = Base::over(columns);
    check::sound(&base).then_some((base, meta))
}

fn too_much<E>(_: E) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, "The index holds more than a snapshot can describe")
}

fn stamps(values: impl ExactSizeIterator<Item = u32>) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(values.len() * STAMP);
    values.for_each(|value| bytes.extend_from_slice(&value.to_le_bytes()));
    bytes
}

fn read_stamps(raw: &[u8], entries: usize) -> Option<impl Iterator<Item = u32> + '_> {
    let (stamps, rest) = raw.as_chunks::<STAMP>();
    (rest.is_empty() && stamps.len() == entries).then(|| stamps.iter().map(|stamp| u32::from_le_bytes(*stamp)))
}

fn head(columns: &Columns, meta: &Meta) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    bytes.extend((columns.len() as u64).to_le_bytes());
    bytes.extend((columns.arena.len() as u64).to_le_bytes());
    bytes.extend(meta.cursor.to_le_bytes());
    bytes.extend(meta.saved_at.to_le_bytes());
    bytes.extend([meta.level, meta.flags]);
    bytes.extend(u16::try_from(meta.roots.len()).map_err(too_much)?.to_le_bytes());
    for root in &meta.roots {
        let path = root.path.to_string_lossy();
        bytes.extend(u16::try_from(path.len()).map_err(too_much)?.to_le_bytes());
        bytes.extend(path.bytes());
        bytes.extend(root.volume);
        bytes.push(root.flags);
        bytes.extend(root.scanned_at.to_le_bytes());
        bytes.extend(root.swept_at.to_le_bytes());
    }
    Ok(bytes)
}

fn read_head(raw: &[u8]) -> Option<(usize, usize, Meta)> {
    let mut rest = Reader(raw);
    let entries = usize::try_from(rest.u64()?).ok().filter(|entries| u32::try_from(*entries).is_ok())?;
    let name_bytes = usize::try_from(rest.u64()?).ok().filter(|bytes| u32::try_from(*bytes).is_ok())?;
    let (cursor, saved_at) = (rest.u64()?, rest.u32()?);
    let [level, flags] = rest.array()?;
    let mut roots = Vec::new();
    for _ in 0..rest.u16()? {
        let length = usize::from(rest.u16()?);
        let path = PathBuf::from(std::str::from_utf8(rest.bytes(length)?).ok()?);
        let (volume, [flags]) = (rest.array()?, rest.array()?);
        roots.push(RootMark { path, volume, flags, scanned_at: rest.u32()?, swept_at: rest.u32()? });
    }
    rest.0.is_empty().then_some((entries, name_bytes, Meta { saved_at, cursor, level, flags, roots }))
}

fn exotic(names: &Exotics) -> io::Result<Vec<u8>> {
    let mut bytes = (names.len() as u32).to_le_bytes().to_vec();
    for (row, name) in names.each() {
        bytes.extend(row.to_le_bytes());
        bytes.extend(u16::try_from(name.len()).map_err(too_much)?.to_le_bytes());
        bytes.extend(name.bytes());
    }
    Ok(bytes)
}

fn read_exotic(raw: &[u8]) -> Option<Exotics> {
    let mut rest = Reader(raw);
    let mut names = Exotics::default();
    for _ in 0..rest.u32()? {
        let (row, length) = (rest.u32()?, usize::from(rest.u16()?));
        names.push(row, std::str::from_utf8(rest.bytes(length)?).ok()?);
    }
    rest.0.is_empty().then_some(names)
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn bytes(&mut self, count: usize) -> Option<&'a [u8]> {
        let (taken, rest) = self.0.split_at_checked(count)?;
        self.0 = rest;
        Some(taken)
    }

    fn array<const N: usize>(&mut self) -> Option<[u8; N]> {
        self.bytes(N)?.try_into().ok()
    }

    fn u16(&mut self) -> Option<u16> {
        self.array().map(u16::from_le_bytes)
    }

    fn u32(&mut self) -> Option<u32> {
        self.array().map(u32::from_le_bytes)
    }

    fn u64(&mut self) -> Option<u64> {
        self.array().map(u64::from_le_bytes)
    }
}
