use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::super::disk::{decode, encode};
use super::super::{Seen, Usage, Used};
use super::{CARGO, MAIN, MONDAY_NOON, README, at, key, lived_in, pick};
use crate::find::context::{DAY, HOUR, MINUTE, Stamp};
use crate::find::kind::Kind;
use crate::find::store::frame::{self, Frame, Tag};
use crate::find::tests::Scratch;
use crate::find::tests::corpus::Noise;
use crate::find::weights::{APPS, FILES, FOLDERS, PAIRS, PREFIXES, PROBATION};

const VERSION_AT: usize = 8;
const DIGEST: usize = 32;
const MOST_ON_DISK: u64 = 300_000;
const FILES_AT: usize = 4;

fn saved(scratch: &Scratch) -> (PathBuf, Frame, Stamp) {
    let file = scratch.file("usage.bin");
    let usage = Usage::load(Some(&file), MONDAY_NOON);
    let now = lived_in(&usage) + DAY;
    assert_eq!(usage.save_if_due(now).ok(), Some(true));
    (file, usage.read(|model| encode(model, now)), now)
}

fn block<'a>(frame: &'a mut Frame, tag: &Tag) -> &'a mut Vec<u8> {
    frame.blocks.iter_mut().find(|(found, _)| found == tag).map(|(_, raw)| raw).unwrap()
}

fn loads_empty(scratch: &Scratch, frame: &Frame) -> bool {
    let file = scratch.file("changed.bin");
    frame::write(&file, frame).unwrap();
    let empty = Usage::load(Some(&file), MONDAY_NOON).read(|model| model.is_empty());
    empty && !file.exists()
}

#[test]
fn learning_survives_save_and_load() {
    let scratch = Scratch::new("usage-round-trip");
    let (file, written, now) = saved(&scratch);
    let again = Usage::load(Some(&file), now);
    assert_eq!(again.read(|model| encode(model, now)), written);
    again.read(|model| {
        assert!(model.known(key(CARGO)).is_some_and(|record| record.opens > 1.9 && record.dayparts[2] == 2));
        assert!(model.file(key(README)).is_some_and(|record| record.gone() && record.file_id == 4_242));
        assert!(
            model.remembered(b"car", key(CARGO), now) > 0.0
                && model.heat(key("/Users/john/GitHub/notchbase/src"), now) > 0.0
        );
        assert_eq!(model.file(key(MAIN)).map(|record| record.skips), Some(1));
        assert!(model.known(key(CARGO)).is_some_and(|record| model.with_app(record, "Terminal").is_some()));
        assert!(!model.book.dirty && model.previous.is_none(), "a fresh load has nothing to write and no last pick");
    });
    assert_eq!(again.matching(&[b"cargo"]), [(key(CARGO), CARGO.into())]);
    assert_eq!(again.save_if_due(now + DAY).ok(), Some(false));
}

#[test]
fn records_match_the_format() {
    let scratch = Scratch::new("usage-format");
    let (_, frame, _) = saved(&scratch);
    let length = |tag: &[u8; 4]| frame.blocks.iter().find(|(found, _)| found == tag).map(|(_, raw)| raw.len());
    let tags: Vec<&[u8]> = frame.blocks.iter().map(|(tag, _)| tag.as_slice()).collect();
    assert_eq!(tags, [&b"HEAD"[..], b"FILE", b"FOLD", b"PREF", b"PAIR", b"APPS", b"PATH"]);
    assert_eq!((frame.kind, length(b"HEAD")), (frame::USAGE, Some(16)));
    assert_eq!((length(b"FILE"), length(b"PREF"), length(b"PAIR")), (Some(5 * 48), Some(4 * 32), Some(2 * 24)));
    assert_eq!(length(b"FOLD").map(|bytes| bytes % 24), Some(0));
    assert_eq!(length(b"APPS"), Some("Xcode\0Terminal\0".len()));
}

#[test]
fn damaged_file_loads_empty() {
    let scratch = Scratch::new("usage-damaged");
    let (file, _, now) = saved(&scratch);
    let bytes = std::fs::read(&file).unwrap();
    for damaged in [bytes[..bytes.len() / 2].to_vec(), Vec::new(), flipped(&bytes, bytes.len() / 3)] {
        std::fs::write(&file, damaged).unwrap();
        assert!(Usage::load(Some(&file), now).read(|model| model.is_empty()));
        assert!(!file.exists());
    }
    assert!(Usage::load(Some(&file), now).read(|model| model.is_empty()), "no file at all");
}

#[test]
fn future_version_loads_empty() {
    let scratch = Scratch::new("usage-future");
    let (file, _, now) = saved(&scratch);
    let mut bytes = std::fs::read(&file).unwrap();
    bytes.truncate(bytes.len() - DIGEST);
    bytes[VERSION_AT] += 1;
    let digest = Sha256::digest(&bytes);
    bytes.extend_from_slice(&digest);
    std::fs::write(&file, bytes).unwrap();
    assert!(Usage::load(Some(&file), now).read(|model| model.is_empty()) && !file.exists());
}

#[test]
fn wrong_counts_load_empty() {
    let scratch = Scratch::new("usage-corrupt");
    let (_, frame, _) = saved(&scratch);
    assert!(!loads_empty(&scratch, &frame), "the untouched frame loads");
    let changed = |change: &dyn Fn(&mut Frame)| {
        let mut frame = frame.clone();
        change(&mut frame);
        frame
    };
    let over_cap = (FILES as u16 + 1).to_le_bytes();
    let damaged = [
        changed(&|frame| block(frame, b"HEAD")[FILES_AT..FILES_AT + 2].copy_from_slice(&over_cap)),
        changed(&|frame| block(frame, b"HEAD")[FILES_AT] -= 1),
        changed(&|frame| block(frame, b"FILE").truncate(47)),
        changed(&|frame| block(frame, b"PREF").push(0)),
        changed(&|frame| block(frame, b"PATH").extend_from_slice(b"/one/too/many\0")),
        changed(&|frame| block(frame, b"PATH").iter_mut().take(4).for_each(|byte| *byte = 0xff)),
        changed(&|frame| block(frame, b"APPS").clear()),
        changed(&|frame| block(frame, b"FILE")[0] ^= 1),
        changed(&|frame| block(frame, b"FILE")[24..28].copy_from_slice(&f32::NAN.to_le_bytes())),
        changed(&|frame| block(frame, b"FOLD")[8..12].copy_from_slice(&(-1.0f32).to_le_bytes())),
        changed(&|frame| block(frame, b"PREF")[12] = 13),
        changed(&|frame| frame.blocks.retain(|(tag, _)| tag != b"PAIR")),
        changed(&|frame| block(frame, b"HEAD").push(0)),
    ];
    for (case, frame) in damaged.iter().enumerate() {
        assert!(loads_empty(&scratch, frame), "case {case}");
    }
}

#[test]
fn orphan_memories_dropped() {
    let scratch = Scratch::new("usage-orphans");
    let (_, mut frame, now) = saved(&scratch);
    block(&mut frame, b"PREF")[16..24].copy_from_slice(&7u64.to_le_bytes());
    block(&mut frame, b"PAIR")[..8].copy_from_slice(&7u64.to_le_bytes());
    let model = decode(&frame).unwrap();
    assert_eq!((model.prefixes.rows().len(), model.pairs.rows().len()), (3, 1));
    assert!(model.prefixes.rows().iter().all(|row| model.file(row.key).is_some()));
    assert!(model.known(key(CARGO)).is_some() && now > MONDAY_NOON);
}

#[test]
fn file_keeps_twelve_typed_bytes() {
    let scratch = Scratch::new("usage-typed");
    let file = scratch.file("usage.bin");
    let usage = Usage::load(Some(&file), MONDAY_NOON);
    usage.picked(&pick(CARGO, "Cargo Manifest for the secret project"), Kind::Text, &at(MONDAY_NOON, None));
    assert_eq!(usage.save_if_due(MONDAY_NOON + 2).ok(), Some(true));
    let stored = frame::read(&file, frame::USAGE).unwrap();
    let holds =
        |needle: &[u8]| stored.blocks.iter().any(|(_, raw)| raw.windows(needle.len()).any(|bytes| bytes == needle));
    assert!(holds(b"cargomanifes") && !holds(b"cargomanifest") && !holds(b"secret") && !holds(b"Manifest"));
    assert!(!holds(b"cargo manife"), "the space is not kept, so twelve letters are");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600);
    }
}

#[test]
fn old_missing_files_expire() {
    let scratch = Scratch::new("usage-expired");
    let (file, _, now) = saved(&scratch);
    assert!(Usage::load(Some(&file), now + 20 * DAY).read(|model| model.file(key(README)).is_some()));
    assert!(Usage::load(Some(&file), now + 40 * DAY).read(|model| model.file(key(README)).is_none()));
}

#[test]
fn full_model_under_300_kb() {
    let scratch = Scratch::new("usage-full");
    let file = scratch.file("usage.bin");
    let usage = Usage::load(Some(&file), MONDAY_NOON);
    let mut noise = Noise::seeded(41);
    let path = |serial: usize, noise: &mut Noise| {
        let project = ["notchbase", "website", "thesis", "taxes-2025", "holiday photos"][noise.below(5)];
        let place = ["src/pages/find", "Documents/drafts", "assets/icons", "notes/weekly", "exports"][noise.below(5)];
        format!(
            "/Users/johnnikitin/Documents/GitHub/{project}/{place}/{:04}/file-{serial:05}.{}",
            noise.below(600),
            ["rs", "md", "pdf", "png"][noise.below(4)]
        )
    };
    let paths: Vec<String> = (0..FILES).map(|serial| path(serial, &mut noise)).collect();
    let opened = &paths[..FILES - PROBATION];
    for (serial, path) in opened.iter().enumerate() {
        let now = MONDAY_NOON + (serial as Stamp) * 7 * MINUTE + noise.below(300) as Stamp;
        let app = format!("App {}", noise.below(APPS));
        let typed: String =
            Path::new(path).file_name().unwrap().to_string_lossy().chars().skip(5).take(2 + noise.below(9)).collect();
        usage.picked(&pick(path, &typed), Kind::Other, &at(now, Some(&app)));
        if serial % 2 == 0 {
            usage.picked(
                &pick(&opened[noise.below(serial + 1)], &typed[..2]),
                Kind::Other,
                &at(now + MINUTE, Some(&app)),
            );
        }
    }
    let end = MONDAY_NOON + FILES as Stamp * 7 * MINUTE + HOUR;
    let edits: Vec<Seen> = paths.iter().map(|path| Seen::Edited { path: path.into(), at: end, plain: true }).collect();
    usage.observe(&edits, end);
    usage.seed(&[Used { path: PathBuf::from(&paths[0]), last: end }], end);
    let filled = usage.read(|model| {
        [
            model.files.len(),
            model.folders.len(),
            model.prefixes.rows().len(),
            model.pairs.rows().len(),
            model.apps.names().len(),
        ]
    });
    assert_eq!(filled, [FILES, FOLDERS, PREFIXES, PAIRS, APPS]);
    assert_eq!(usage.save_if_due(end + DAY).ok(), Some(true));
    let (on_disk, raw) = (
        std::fs::metadata(&file).unwrap().len(),
        usage.read(|model| encode(model, end)).blocks.iter().map(|(_, raw)| raw.len()).sum::<usize>(),
    );
    println!("usage.bin at every cap: {on_disk} bytes on disk, {raw} bytes raw");
    assert!(on_disk <= MOST_ON_DISK, "{on_disk} bytes");
    let again = Usage::load(Some(&file), end + DAY);
    assert_eq!(again.read(|model| encode(model, end)), usage.read(|model| encode(model, end)));
}

fn flipped(bytes: &[u8], at: usize) -> Vec<u8> {
    let mut bytes = bytes.to_vec();
    bytes[at] ^= 0x10;
    bytes
}
