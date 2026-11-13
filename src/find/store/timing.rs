use std::fs;
use std::time::Instant;

use crate::find::model::{Entry, EntryId, Live, Room, flag};
use crate::find::tests::Scratch;
use crate::find::tests::corpus::{EPOCH, Noise, SPAN};

use super::{INDEX_FILE, Meta, index, load, save};

const SIZES: [(usize, usize, f64); 2] = [(1_450_000, 6, 250.0), (6_700_000, 11, 1_500.0)];
const ROOMY: Room = Room { cap: usize::MAX, level: 0, boot: None };
const FOLDER_SHARE: u64 = 5;
const ANYWHERE_SHARE: u64 = 3;
const NEAR: usize = 64;
const CREATED_EARLIER_SHARE: u64 = 30;
const STEMS: [&str; 12] =
    ["report", "Notes", "index", "main", "Config", "IMG", "draft", "Invoice", "README", "budget", "Sketch", "export"];
const ENDINGS: [&str; 8] = ["txt", "md", "rs", "pdf", "PNG", "json", "toml", "mov"];
const ACCENTED_SHARE: u64 = 50;
const ACCENTED: &str = "R\u{e9}sum\u{e9}";
const STEADY_BYTES_PER_ENTRY: f64 = 40.0;
const MEGABYTE: f64 = 1_048_576.0;

fn milliseconds(since: Instant) -> f64 {
    since.elapsed().as_secs_f64() * 1_000.0
}

fn rebuild(live: &mut Live) -> f64 {
    let started = Instant::now();
    let rebuilt = live.rebuilt(&ROOMY);
    live.adopt(rebuilt);
    milliseconds(started)
}

fn folders_of(live: &Live) -> Vec<EntryId> {
    (0..live.len() as EntryId).filter(|id| live.flags(*id) & flag::DIR != 0).collect()
}

fn grow(live: &mut Live, noise: &mut Noise, folders: &mut Vec<EntryId>) {
    let near = folders.len() - 1 - noise.below(folders.len().min(NEAR));
    let parent = folders[if noise.roll().is_multiple_of(ANYWHERE_SHARE) { noise.below(folders.len()) } else { near }];
    let stem = if noise.roll().is_multiple_of(ACCENTED_SHARE) { ACCENTED } else { STEMS[noise.below(STEMS.len())] };
    let modified = EPOCH + (noise.roll() % u64::from(SPAN)) as u32;
    let created = if noise.roll().is_multiple_of(CREATED_EARLIER_SHARE) {
        modified - noise.below(SPAN as usize) as u32
    } else {
        modified
    };
    let serial = live.len();
    if noise.roll().is_multiple_of(FOLDER_SHARE) {
        let name = format!("{stem}-{serial}");
        folders.push(live.add(Some(parent), Entry { name: &name, modified, created, flags: flag::DIR }));
    } else {
        let name = format!("{stem}-{serial}.{}", ENDINGS[noise.below(ENDINGS.len())]);
        live.add(Some(parent), Entry { name: &name, modified, created, flags: 0 });
    }
}

fn first_scan(entries: usize) -> (Live, Vec<f64>, f64) {
    let mut noise = Noise::seeded(entries as u64);
    let mut live = Live::empty();
    let mut folders = vec![live.add(None, Entry { name: "/", modified: EPOCH, created: EPOCH, flags: flag::DIR })];
    let (mut merges, mut added_row) = (Vec::new(), 0.0);
    while live.len() < entries {
        grow(&mut live, &mut noise, &mut folders);
        if live.wants_rebuild() {
            added_row = live.added_bytes() as f64 / (live.len() - live.base().len()) as f64;
            merges.push(rebuild(&mut live));
            folders = folders_of(&live);
        }
    }
    merges.push(rebuild(&mut live));
    (live, merges, added_row)
}

#[test]
#[ignore = "prints rebuild, save and load timings for millions of names; run with --release"]
fn first_scan_within_budget() {
    for (entries, rebuilds, merge_budget) in SIZES {
        let scratch = Scratch::new("store-timing");
        let scanned = Instant::now();
        let (mut live, merges, added_row) = first_scan(entries);
        let scan = milliseconds(scanned);
        let steady = live.base().columns.bytes() as f64;
        let copy = rebuild(&mut live);
        let saving = Instant::now();
        save(scratch.dir(), &live, &Meta::default()).unwrap();
        let (saved, on_disk) = (milliseconds(saving), fs::metadata(scratch.file(INDEX_FILE)).unwrap().len() as f64);
        let loading = Instant::now();
        let (loaded, _) = load(scratch.dir()).expect("the snapshot we just wrote");
        let read = milliseconds(loading);
        let slowest = merges.iter().copied().fold(0.0, f64::max);
        println!("{entries} entries: {} rebuilds during the scan, in ms: {merges:.0?}", merges.len());
        println!("  scan and merges {scan:.0} ms, slowest merge {slowest:.0} ms, settled rebuild {copy:.0} ms");
        println!("  memory {:.1} MB, {:.1} B per entry", steady / MEGABYTE, steady / entries as f64);
        println!("  a row not yet merged {added_row:.1} B");
        println!("  snapshot {:.1} MB, {:.1} B per entry", on_disk / MEGABYTE, on_disk / entries as f64);
        for (tag, raw) in index::encode(live.base(), &Meta::default()).unwrap().blocks {
            let (name, packed) = (String::from_utf8_lossy(&tag).into_owned(), lz4_flex::block::compress(&raw).len());
            println!(
                "    {name} {:.2} B per entry packed, {:.2} raw",
                packed as f64 / entries as f64,
                raw.len() as f64 / entries as f64
            );
        }
        println!("  save {saved:.0} ms, load {read:.0} ms");
        assert!(loaded.base() == live.base(), "what was loaded is what was saved");
        assert_eq!(merges.len(), rebuilds);
        assert!(steady / entries as f64 <= STEADY_BYTES_PER_ENTRY);
        assert!(cfg!(debug_assertions) || slowest.max(copy) <= merge_budget, "a rebuild took {slowest:.0} ms");
    }
}
