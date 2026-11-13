use std::fs;

use sha2::{Digest, Sha256};

use crate::find::model::base::Base;
use crate::find::model::columns::{Columns, NO_PARENT};
use crate::find::model::{Entry, EntryId, Live, flag};
use crate::find::store::frame::{self, Frame, Tag};
use crate::find::store::{INDEX_FILE, Meta, index, load, save};
use crate::find::tests::Scratch;

use super::{marks, sample};

const DIGEST: usize = 32;
const VERSION_AT: usize = 8;
const SYSTEM_AT: usize = 13;
const SYSTEMS: u8 = 3;
const ROOT_COUNT_AT: usize = 30;
const INSIDE_A_NAME: usize = 4;
const SORTED: [(EntryId, &str); 6] =
    [(NO_PARENT, "/"), (0, "Apps"), (0, "R\u{e9}sum\u{e9}.pdf"), (0, "users"), (1, "Safari.app"), (3, "john")];

fn base(rows: &[(EntryId, &str)]) -> Base {
    let mut columns = Columns::default();
    for (parent, name) in rows {
        columns.push(*parent, Entry { name, modified: 9, created: 4, flags: flag::DIR });
    }
    Base::over(columns)
}

fn loads(scratch: &Scratch, frame: &Frame) -> bool {
    let file = scratch.file(INDEX_FILE);
    frame::write(&file, frame).unwrap();
    let loaded = load(scratch.dir()).is_some();
    assert_eq!(file.exists(), loaded, "a refused snapshot is removed");
    loaded
}

fn loads_rows(scratch: &Scratch, rows: &[(EntryId, &str)]) -> bool {
    loads(scratch, &index::encode(&base(rows), &Meta::default()).unwrap())
}

fn with_block(tag: Tag, change: impl FnOnce(&mut Vec<u8>)) -> Frame {
    let mut frame = index::encode(&base(&SORTED), &marks()).unwrap();
    let block = frame.blocks.iter_mut().find(|(found, _)| *found == tag).expect("every snapshot has this block");
    change(&mut block.1);
    frame
}

fn refused_bytes(scratch: &Scratch, bytes: &[u8]) -> bool {
    let file = scratch.file(INDEX_FILE);
    fs::write(&file, bytes).unwrap();
    load(scratch.dir()).is_none() && !file.exists()
}

fn resealed(mut bytes: Vec<u8>, change: impl FnOnce(&mut [u8])) -> Vec<u8> {
    bytes.truncate(bytes.len() - DIGEST);
    change(&mut bytes);
    let digest = Sha256::digest(&bytes);
    bytes.extend_from_slice(&digest);
    bytes
}

fn written(scratch: &Scratch) -> Vec<u8> {
    save(scratch.dir(), &sample(41), &marks()).unwrap();
    fs::read(scratch.file(INDEX_FILE)).unwrap()
}

#[test]
fn damaged_snapshot_is_removed() {
    let scratch = Scratch::new("store-damaged");
    let bytes = written(&scratch);
    assert!(refused_bytes(&scratch, &bytes[..bytes.len() - 1]));
    assert!(refused_bytes(&scratch, &bytes[..bytes.len() / 2]));
    assert!(refused_bytes(&scratch, &[]));
    for at in [0, VERSION_AT, SYSTEM_AT, bytes.len() / 3, bytes.len() - DIGEST - 1, bytes.len() - 1] {
        let mut flipped = bytes.clone();
        flipped[at] ^= 0x04;
        assert!(refused_bytes(&scratch, &flipped), "a flipped bit at {at}");
    }
    assert!(!refused_bytes(&scratch, &bytes), "the file itself was fine");
}

#[test]
fn foreign_snapshot_is_refused() {
    let scratch = Scratch::new("store-foreign");
    let bytes = written(&scratch);
    assert!(!refused_bytes(&scratch, &resealed(bytes.clone(), |_| {})), "resealing alone changes nothing");
    assert!(refused_bytes(&scratch, &resealed(bytes.clone(), |bytes| bytes[VERSION_AT] += 1)));
    assert!(refused_bytes(
        &scratch,
        &resealed(bytes.clone(), |bytes| bytes[SYSTEM_AT] = bytes[SYSTEM_AT] % SYSTEMS + 1)
    ));
    let learned = Frame { kind: frame::USAGE, ..index::encode(Live::empty().base(), &Meta::default()).unwrap() };
    assert!(!loads(&scratch, &learned), "the other file's kind");
}

#[test]
fn unordered_rows_are_refused() {
    let scratch = Scratch::new("store-mis-sorted");
    assert!(loads_rows(&scratch, &SORTED));
    let swapped = |left: usize, right: usize| {
        let mut rows = SORTED;
        rows.swap(left, right);
        rows
    };
    assert!(!loads_rows(&scratch, &swapped(1, 3)), "siblings out of order");
    assert!(!loads_rows(&scratch, &swapped(1, 2)), "siblings out of order where the fold decides");
    assert!(!loads_rows(&scratch, &swapped(3, 4)), "a parent column that steps back");
    assert!(!loads_rows(&scratch, &[(0, "self"), (0, "child")]), "a row that is its own parent");
    assert!(!loads_rows(&scratch, &[(NO_PARENT, "/"), (2, "early"), (0, "late")]), "a child before its parent");
    assert!(!loads_rows(&scratch, &[(NO_PARENT, "/"), (7, "orphan")]), "a parent that is not there");
    assert!(loads_rows(&scratch, &[(NO_PARENT, "/"), (NO_PARENT, "/Volumes/Work"), (0, "a"), (1, "a")]));
}

#[test]
fn block_against_head_is_refused() {
    let scratch = Scratch::new("store-oversize");
    assert!(loads(&scratch, &with_block(index::HEAD, |_| {})));
    for tag in [index::PARENTS, index::MODIFIED, index::CREATED_BEFORE, index::FLAGS, index::NAMES, index::CAPITALS] {
        assert!(!loads(&scratch, &with_block(tag, |block| block.push(0))), "{tag:?} one byte over");
        assert!(!loads(&scratch, &with_block(tag, |block| block.truncate(block.len() - 1))), "{tag:?} one byte short");
        assert!(!loads(&scratch, &with_block(tag, Vec::clear)), "{tag:?} empty");
        let mut without = with_block(tag, |_| {});
        without.blocks.retain(|(found, _)| *found != tag);
        assert!(!loads(&scratch, &without), "{tag:?} missing");
    }
    assert!(!loads(&scratch, &with_block(index::HEAD, |head| head[0] += 1)), "one more entry than there is");
    assert!(!loads(&scratch, &with_block(index::HEAD, |head| head[..8].fill(0xff))), "more entries than fit");
    assert!(!loads(&scratch, &with_block(index::HEAD, |head| head[8] += 1)), "a longer arena than there is");
    assert!(!loads(&scratch, &with_block(index::HEAD, |head| head.push(0))), "bytes after the last root");
    assert!(!loads(&scratch, &with_block(index::HEAD, |head| head.truncate(head.len() - 1))), "a root cut short");
    assert!(!loads(&scratch, &with_block(index::HEAD, |head| head[ROOT_COUNT_AT] += 1)), "more roots than follow");
}

#[test]
fn broken_names_are_refused() {
    let scratch = Scratch::new("store-names");
    assert!(!loads(&scratch, &with_block(index::NAMES, |names| names[INSIDE_A_NAME] = 0)), "one name too many");
    assert!(!loads(&scratch, &with_block(index::NAMES, |names| names[0] = b'x')), "no opening end of name");
    let last = |names: &mut Vec<u8>| *names.last_mut().unwrap() = b'x';
    assert!(!loads(&scratch, &with_block(index::NAMES, last)), "the last name never ends");
    assert!(!loads(&scratch, &with_block(index::EXOTIC, |table| table.truncate(4))), "a count with no rows");
    assert!(!loads(&scratch, &with_block(index::EXOTIC, |table| table[..4].fill(0))), "rows nobody counted");
    assert!(!loads(&scratch, &with_block(index::EXOTIC, |table| *table = vec![0; 4])), "a flag without its name");
    assert!(!loads(&scratch, &with_block(index::EXOTIC, |table| table[4] = 1)), "a name filed under another row");
    assert!(!loads(&scratch, &with_block(index::EXOTIC, |table| table[10] = 0xff)), "a name that is not text");
    assert!(!loads(&scratch, &with_block(index::FLAGS, |flags| flags[2] &= !flag::EXOTIC)), "a name without its flag");
    let mut twice = with_block(index::EXOTIC, |_| {});
    twice.blocks.push((*b"WHAT", vec![1, 2, 3]));
    assert!(loads(&scratch, &twice), "a block from the future is skipped");
}
