//! Writes the model to usage.bin and reads it back, one fixed-size record per file, folder, typed memory and sequence.
//! A file whose counts or lengths do not add up loads as nothing.

use std::path::Path;

use crate::find::context::{Daypart, Stamp};
use crate::find::key::{Key, key_of};
use crate::find::store::frame::{Frame, Tag, USAGE};
use crate::find::weights::{APPS, FILES, FOLDERS, PAIRS, PREFIX_BYTES, PREFIXES, SKIPS_OUT};

use super::habits::Apps;
use super::model::Model;
use super::pairs::{Pair, Pairs};
use super::prefix::{Memory, Prefixes, Typed};
use super::record::{FileRecord, FolderRecord, SLOTS, state};

const HEAD: Tag = *b"HEAD";
const FILE: Tag = *b"FILE";
const FOLD: Tag = *b"FOLD";
const PREF: Tag = *b"PREF";
const PAIR: Tag = *b"PAIR";
const APPS_TAG: Tag = *b"APPS";
const PATH: Tag = *b"PATH";
const HEAD_BYTES: usize = 16;
const FILE_BYTES: usize = 48;
const FOLD_BYTES: usize = 24;
const PREF_BYTES: usize = 32;
const PAIR_BYTES: usize = 24;
const END: u8 = 0;

pub(super) fn encode(model: &Model, now: Stamp) -> Frame {
    let mut files: Vec<(&FileRecord, &str)> = model.files.each().collect();
    let mut folders: Vec<(&FolderRecord, &str)> = model.folders.each().collect();
    files.sort_by_key(|(_, path)| *path);
    folders.sort_by_key(|(_, path)| *path);
    let (prefixes, pairs, apps) = (model.prefixes.rows(), model.pairs.rows(), model.apps.names());
    let mut head = Vec::with_capacity(HEAD_BYTES);
    head.extend(now.to_le_bytes());
    for count in [files.len(), folders.len(), prefixes.len(), pairs.len()] {
        head.extend((count as u16).to_le_bytes());
    }
    head.push(apps.len() as u8);
    head.resize(HEAD_BYTES, 0);
    let paths = files.iter().map(|(_, path)| *path).chain(folders.iter().map(|(_, path)| *path));
    let blocks = vec![
        (HEAD, head),
        (FILE, files.iter().flat_map(|(record, _)| file_bytes(record)).collect()),
        (FOLD, folders.iter().flat_map(|(record, _)| folder_bytes(record)).collect()),
        (PREF, prefixes.iter().flat_map(prefix_bytes).collect()),
        (PAIR, pairs.iter().flat_map(pair_bytes).collect()),
        (APPS_TAG, ended(apps.iter().map(|name| &**name))),
        (PATH, ended(paths)),
    ];
    Frame { kind: USAGE, blocks }
}

pub(super) fn decode(frame: &Frame) -> Option<Model> {
    let block = |tag: Tag| frame.blocks.iter().find(|(found, _)| *found == tag).map(|(_, raw)| raw.as_slice());
    let head = block(HEAD).filter(|head| head.len() == HEAD_BYTES)?;
    let count = |at: usize| usize::from(u16::from_le_bytes([head[at], head[at + 1]]));
    let (files, folders, prefixes, pairs, apps) = (count(4), count(6), count(8), count(10), usize::from(head[12]));
    if files > FILES || folders > FOLDERS || prefixes > PREFIXES || pairs > PAIRS || apps > APPS {
        return None;
    }
    let records = |tag: Tag, size: usize, count: usize| block(tag).filter(|raw| raw.len() == size * count);
    let names = texts(block(APPS_TAG)?, apps)?;
    let paths = texts(block(PATH)?, files + folders)?;
    let mut model =
        Model { apps: Apps::from_names(names.iter().map(|name| (*name).into()).collect()), ..Model::default() };
    for (raw, path) in records(FILE, FILE_BYTES, files)?.as_chunks::<FILE_BYTES>().0.iter().zip(&paths) {
        model.files.put(file_from(raw, apps).filter(|record| record.key == key_of(Path::new(path)))?, path);
    }
    for (raw, path) in records(FOLD, FOLD_BYTES, folders)?.as_chunks::<FOLD_BYTES>().0.iter().zip(&paths[files..]) {
        model.folders.put(folder_from(raw).filter(|record| record.key == key_of(Path::new(path)))?, path);
    }
    if model.files.len() != files || model.folders.len() != folders {
        return None;
    }
    let known = |key: Key| model.files.get(key).is_some();
    let remembered: Option<Vec<Memory>> =
        records(PREF, PREF_BYTES, prefixes)?.as_chunks::<PREF_BYTES>().0.iter().map(|raw| prefix_from(raw)).collect();
    let followed: Option<Vec<Pair>> =
        records(PAIR, PAIR_BYTES, pairs)?.as_chunks::<PAIR_BYTES>().0.iter().map(|raw| pair_from(raw)).collect();
    let (mut remembered, mut followed) = (remembered?, followed?);
    remembered.retain(|row| known(row.key));
    followed.retain(|row| known(row.before) && known(row.after));
    model.prefixes = Prefixes::from_rows(remembered);
    model.pairs = Pairs::from_rows(followed);
    Some(model)
}

fn ended<'a>(texts: impl Iterator<Item = &'a str>) -> Vec<u8> {
    texts.flat_map(|text| text.bytes().chain([END])).collect()
}

fn texts(raw: &[u8], count: usize) -> Option<Vec<&str>> {
    let body = match raw.split_last() {
        Some((last, body)) if *last == END => body,
        Some(_) => return None,
        None => return (count == 0).then(Vec::new),
    };
    let texts: Option<Vec<&str>> = body.split(|byte| *byte == END).map(|text| std::str::from_utf8(text).ok()).collect();
    texts.filter(|texts| texts.len() == count && texts.iter().all(|text| !text.is_empty()))
}

fn file_bytes(record: &FileRecord) -> [u8; FILE_BYTES] {
    let mut raw = [0; FILE_BYTES];
    raw[..8].copy_from_slice(&record.key.0.to_le_bytes());
    raw[8..16].copy_from_slice(&record.file_id.to_le_bytes());
    raw[16..20].copy_from_slice(&record.last_open.to_le_bytes());
    raw[20..24].copy_from_slice(&record.last_edit.to_le_bytes());
    raw[24..28].copy_from_slice(&record.opens.to_le_bytes());
    raw[28..32].copy_from_slice(&record.edits.to_le_bytes());
    raw[32..40].copy_from_slice(&record.dayparts);
    raw[40..42].copy_from_slice(&record.apps);
    raw[42..44].copy_from_slice(&record.app_weight);
    raw[44] = record.skips;
    raw[45] = record.flags;
    raw[46..48].copy_from_slice(&record.day.to_le_bytes());
    raw
}

fn file_from(raw: &[u8], apps: usize) -> Option<FileRecord> {
    let mut record = FileRecord::new(Key(whole(raw, 0)?));
    record.file_id = whole(raw, 8)?;
    (record.last_open, record.last_edit) = (stamp(raw, 16)?, stamp(raw, 20)?);
    (record.opens, record.edits) = (amount(raw, 24)?, amount(raw, 28)?);
    record.dayparts = raw.get(32..32 + Daypart::COUNT)?.try_into().ok()?;
    record.apps = raw.get(40..40 + SLOTS)?.try_into().ok()?;
    record.app_weight = raw.get(42..42 + SLOTS)?.try_into().ok()?;
    for slot in 0..SLOTS {
        if usize::from(record.apps[slot]) >= apps {
            (record.apps[slot], record.app_weight[slot]) = (0, 0);
        }
    }
    record.skips = (*raw.get(44)?).min(SKIPS_OUT);
    record.flags = *raw.get(45)? & state::KNOWN;
    record.day = u16::from_le_bytes(raw.get(46..48)?.try_into().ok()?);
    Some(record)
}

fn folder_bytes(record: &FolderRecord) -> [u8; FOLD_BYTES] {
    let mut raw = [0; FOLD_BYTES];
    raw[..8].copy_from_slice(&record.key.0.to_le_bytes());
    raw[8..12].copy_from_slice(&record.heat.to_le_bytes());
    raw[12..16].copy_from_slice(&record.opens.to_le_bytes());
    raw[16..20].copy_from_slice(&record.last.to_le_bytes());
    raw
}

fn folder_from(raw: &[u8]) -> Option<FolderRecord> {
    Some(FolderRecord {
        key: Key(whole(raw, 0)?),
        heat: amount(raw, 8)?,
        opens: amount(raw, 12)?,
        last: stamp(raw, 16)?,
    })
}

fn prefix_bytes(row: &Memory) -> [u8; PREF_BYTES] {
    let mut raw = [0; PREF_BYTES];
    raw[..row.typed.text().len()].copy_from_slice(row.typed.text());
    raw[PREFIX_BYTES] = row.typed.text().len() as u8;
    raw[16..24].copy_from_slice(&row.key.0.to_le_bytes());
    raw[24..28].copy_from_slice(&row.weight.to_le_bytes());
    raw[28..32].copy_from_slice(&row.last.to_le_bytes());
    raw
}

fn prefix_from(raw: &[u8]) -> Option<Memory> {
    let typed = raw.get(..usize::from(*raw.get(PREFIX_BYTES)?)).filter(|text| text.len() <= PREFIX_BYTES)?;
    Some(Memory { typed: Typed::of(typed), key: Key(whole(raw, 16)?), weight: amount(raw, 24)?, last: stamp(raw, 28)? })
}

fn pair_bytes(row: &Pair) -> [u8; PAIR_BYTES] {
    let mut raw = [0; PAIR_BYTES];
    raw[..8].copy_from_slice(&row.before.0.to_le_bytes());
    raw[8..16].copy_from_slice(&row.after.0.to_le_bytes());
    raw[16..20].copy_from_slice(&row.weight.to_le_bytes());
    raw[20..24].copy_from_slice(&row.last.to_le_bytes());
    raw
}

fn pair_from(raw: &[u8]) -> Option<Pair> {
    Some(Pair {
        before: Key(whole(raw, 0)?),
        after: Key(whole(raw, 8)?),
        weight: amount(raw, 16)?,
        last: stamp(raw, 20)?,
    })
}

fn whole(raw: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(raw.get(at..at + 8)?.try_into().ok()?))
}

fn stamp(raw: &[u8], at: usize) -> Option<Stamp> {
    Some(Stamp::from_le_bytes(raw.get(at..at + 4)?.try_into().ok()?))
}

fn amount(raw: &[u8], at: usize) -> Option<f32> {
    let amount = f32::from_le_bytes(raw.get(at..at + 4)?.try_into().ok()?);
    (amount.is_finite() && amount >= 0.0).then_some(amount)
}
