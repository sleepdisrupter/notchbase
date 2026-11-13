use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::{Duration, Instant};

use crate::find::context::{Context, HOUR};
use crate::find::model::{Entry, EntryId, Live, Room, flag, tier, with_tier};
use crate::find::query::{self, Stop};
use crate::find::shared::Ask;
use crate::find::store::{self, Meta};
use crate::find::usage::Usage;

use super::Scratch;
use super::corpus::{EPOCH, Noise, Planted, SPAN, grown, plant};

const SIZES: [usize; 2] = [1_450_000, 6_700_000];
const SIZES_FROM: &str = "NOTCHBASE_FIND_TIMINGS";
const HERE: usize = 1_450_000;
const RUNS: usize = 40;
const SEED: u64 = 77;
const PLISTS_ONE_IN: usize = 12;
const JUNK_STARTS_ONE_IN: usize = 8;
const JUNK_TIERS: [u8; 6] = [1, 1, 1, 2, 2, 3];
const HOME: &str = "/Users/john";
const NOW: u32 = EPOCH + SPAN;
const SINGLE: [&str; 6] = ["e", "ca", "plist", ".rs", "today", "xtxtx"];
const SHORTENED: [&str; 4] = ["rpt", "cfg", "bkp", "invc"];
const SEVERAL: [&str; 3] = ["notchbase readme", "e a", "readme e"];
const APPS_EACH: usize = 200;
const START_MENU: &str = "/ProgramData/Microsoft/Windows/Start Menu/Programs";
const SYSTEM: u8 = 1;

struct Limits {
    half: Duration,
    nearly_all: Duration,
    several: Option<Duration>,
}

const SMALL: Limits = Limits {
    half: Duration::from_millis(5),
    nearly_all: Duration::from_millis(15),
    several: Some(Duration::from_millis(25)),
};
const LARGE: Limits = Limits { half: Duration::from_millis(10), nearly_all: Duration::from_millis(45), several: None };

fn timed<T>(work: impl FnOnce() -> T) -> (T, Duration) {
    let started = Instant::now();
    (work(), started.elapsed())
}

fn corpus(entries: usize) -> Live {
    let mut live = grown(SEED, entries);
    let mut noise = Noise::seeded(SEED);
    for id in 1..live.len() as EntryId {
        let (flags, above) = (live.flags(id), tier(live.flags(live.parent(id).unwrap_or_default())));
        let turns = above == 0 && flags & flag::DIR != 0 && noise.below(JUNK_STARTS_ONE_IN) == 0;
        let level = if turns { JUNK_TIERS[noise.below(JUNK_TIERS.len())] } else { above };
        live.reflag(id, with_tier(flags, level));
    }
    for nth in 0..entries / PLISTS_ONE_IN {
        let parent = noise.below(entries) as EntryId;
        let folder = if live.flags(parent) & flag::DIR != 0 { parent } else { live.parent(parent).unwrap_or_default() };
        let flags = with_tier(0, tier(live.flags(folder)));
        live.add(Some(folder), Entry { name: &format!("Info-{nth}.plist"), modified: EPOCH, created: EPOCH, flags });
    }
    for path in
        ["/Users/john/Documents/GitHub/notchbase/README.md", "/Users/john/Documents/GitHub/notchbase/Cargo.toml"]
    {
        plant(&mut live, &Planted { path, modified: NOW - HOUR, created: EPOCH, flags: 0 });
    }
    for nth in 0..APPS_EACH {
        let bundle = format!("/Applications/Budget Planner {nth}.app/");
        let shortcut = format!("{START_MENU}/Report Reader {nth}/Report Reader {nth}.lnk");
        plant(&mut live, &Planted { path: &bundle, modified: EPOCH, created: EPOCH, flags: 0 });
        plant(&mut live, &Planted { path: &shortcut, modified: EPOCH, created: EPOCH, flags: with_tier(0, SYSTEM) });
    }
    live
}

fn search(live: &Live, text: &str) -> (Duration, Duration) {
    let ask = Ask { generation: 1, text: Arc::from(text), context: Context::at(NOW, 0, None) };
    let (latest, usage) = (AtomicU64::new(1), Usage::load(None, 0));
    let mut scratch = query::Scratch::new(Some(PathBuf::from(HOME)));
    scratch.launchers.refresh(live, &Stop { latest: &latest, mine: 1 }, &mut vec![0; live.len()]);
    let mut rows = 0;
    let once = |_| {
        let (answer, took) = timed(|| query::run(live, &usage, &ask, &mut scratch, &Stop { latest: &latest, mine: 1 }));
        rows = answer.map_or(0, |answer| answer.rows.len());
        took
    };
    let mut took: Vec<Duration> = (0..RUNS).map(once).collect();
    took.sort_unstable();
    let (best, half, nearly_all) = (took[0], took[RUNS / 2], took[RUNS * 99 / 100]);
    println!("{text:>18}   p50 {half:>9.2?}   p99 {nearly_all:>9.2?}   best {best:>9.2?}   {rows} rows");
    (half, nearly_all)
}

#[test]
#[ignore = "prints timings for a million names; run with --release"]
fn million_names_in_time() {
    let asked = std::env::var(SIZES_FROM).ok();
    let chosen =
        asked.map(|sizes| sizes.split(',').filter_map(|size| size.trim().parse().ok()).collect::<Vec<usize>>());
    let mut late: Vec<String> = Vec::new();
    for entries in chosen.unwrap_or_else(|| SIZES.to_vec()) {
        let limits = if entries <= HERE { SMALL } else { LARGE };
        let (mut live, grew) = timed(|| corpus(entries));
        let users = (0..live.len() as EntryId).filter(|id| tier(live.flags(*id)) == 0).count();
        println!("{} entries ({users} of them user files), built in {grew:.1?}", live.len());
        let (rebuilt, rebuild) = timed(|| live.rebuilt(&Room { cap: usize::MAX, level: 0, boot: Some(0) }));
        live.adopt(rebuilt);
        let folder = Scratch::new("timings");
        let (saved, save) = timed(|| store::save(folder.dir(), &live, &Meta::default()));
        let (loaded, load) = timed(|| store::load(folder.dir()));
        let size = std::fs::metadata(folder.file(store::INDEX_FILE)).map_or(0, |file| file.len());
        println!(
            "everything rebuilt at once {rebuild:.1?}   save {save:.1?} ({saved:?}, {size} bytes)   load {load:.1?} ({})",
            loaded.is_some()
        );
        let (first, mut marks, mut apps) = (AtomicU64::new(0), vec![0; live.len()], query::Launchers::default());
        let (_, listed) = timed(|| apps.refresh(&live, &Stop { latest: &first, mine: 0 }, &mut marks));
        println!("{} apps listed when Find opens {listed:.2?}", apps.known().len());
        let mut halves = Vec::new();
        for text in SINGLE {
            let (half, nearly_all) = search(&live, text);
            halves.push(half);
            late.extend((nearly_all > limits.nearly_all).then(|| format!("{entries}: {text:?} took {nearly_all:.2?}")));
        }
        halves.sort_unstable();
        let typical = halves[halves.len() / 2];
        late.extend((typical > limits.half).then(|| format!("{entries}: a typical word took {typical:.2?}")));
        for text in SEVERAL {
            let (_, nearly_all) = search(&live, text);
            let slow = limits.several.is_some_and(|most| nearly_all > most);
            late.extend(slow.then(|| format!("{entries}: {text:?} took {nearly_all:.2?}")));
        }
        for text in SHORTENED {
            search(&live, text);
        }
    }
    assert!(cfg!(debug_assertions) || late.is_empty(), "slower than the targets: {late:#?}");
}
