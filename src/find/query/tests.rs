use std::path::Path;
use std::sync::Arc;

use super::collect::JUNK_BUDGET;
use super::*;
use crate::find::key::key_of;
use crate::find::kind::Kind;
use crate::find::model::{Entry, flag, with_tier};
use crate::find::tests::corpus::{EPOCH, Planted, from_paths, plant};
use crate::find::view::{FindRow, Why};

const HOME: &str = "/Users/john";

fn asked(live: &Live, text: &str, latest: u64) -> Option<Answer> {
    looked(live, text, latest, false)
}

fn asked_for_everything(live: &Live, text: &str) -> Option<Answer> {
    looked(live, text, 1, true)
}

fn looked(live: &Live, text: &str, latest: u64, everything: bool) -> Option<Answer> {
    let ask = Ask { generation: 1, text: Arc::from(text), context: Context::at(EPOCH, 0, None) };
    let (latest, usage) = (AtomicU64::new(latest), Usage::load(None, 0));
    let mut scratch = Scratch::new(Some(PathBuf::from(HOME)));
    scratch.everything = everything;
    run(live, &usage, &ask, &mut scratch, &Stop { latest: &latest, mine: 1 })
}

fn names(live: &Live, text: &str) -> Vec<String> {
    let answer = asked(live, text, 1).expect("nothing newer was asked");
    assert_eq!((answer.generation, &*answer.query), (1, text));
    answer.rows.iter().map(|row| row.name.to_string()).collect()
}

fn lit(row: &FindRow) -> Vec<(u16, u16)> {
    row.marks.iter().map(|lit| (lit.start, lit.end)).collect()
}

fn desk() -> Live {
    from_paths(&[
        "/Users/john/Documents/Cargo.toml",
        "/Users/john/Documents/cargo-notes.md",
        "/Users/john/Car.png",
        "/Users/john/Readme.md",
        "/Applications/Calendar.app/",
    ])
}

fn junk_drawer(files: usize) -> Live {
    let mut live = from_paths(&["/Users/john/Library/Caches/store/"]);
    let store = live.resolve(Path::new("/Users/john/Library/Caches/store")).unwrap();
    let cached = with_tier(0, 3);
    for nth in 0..files {
        live.add(
            Some(store),
            Entry { name: &format!("cache-{nth}.tmp"), modified: EPOCH, created: EPOCH, flags: cached },
        );
    }
    live
}

#[test]
fn start_before_inside() {
    let live = from_paths(&["/Users/john/scar.txt", "/Users/john/Documents/Cargo.toml", "/Users/john/Car.png"]);
    assert_eq!(names(&live, "car"), ["Car.png", "Cargo.toml", "scar.txt"]);
    assert_eq!(names(&live, "CAR"), ["Car.png", "Cargo.toml", "scar.txt"], "capitals never filter");
    assert_eq!(names(&desk(), "md  notes"), ["cargo-notes.md"], "every word has to be there");
    assert_eq!(names(&desk(), "zebra"), Vec::<String>::new());
}

#[test]
fn empty_offers_newest() {
    let answer = asked(&desk(), "  ", 1).unwrap();
    let names: Vec<&str> = answer.rows.iter().map(|row| &*row.name).collect();
    assert_eq!((names, answer.reading, answer.more), (vec!["Car.png"], None, false));
    assert_eq!(answer.rows[0].why, Some(Why::Added(EPOCH)));
}

#[test]
fn buried_never_returned() {
    let mut live = desk();
    let gone = live.resolve(Path::new("/Users/john/Car.png")).unwrap();
    live.bury(gone);
    assert_eq!(names(&live, "car"), ["Cargo.toml", "cargo-notes.md", "Calendar"], "then a guess, never the dead");
}

#[test]
fn newer_generation_cancels() {
    assert_eq!(asked(&desk(), "car", 2), None);
    assert_eq!(asked(&desk(), "folders today", 2), None);
    assert_eq!(asked(&desk(), "", 2), None);
}

#[test]
fn at_most_a_screenful() {
    let paths: Vec<String> = (0..SHOWN + 15).map(|file| format!("/Users/john/report-{file:03}.txt")).collect();
    let live = from_paths(&paths.iter().map(String::as_str).collect::<Vec<_>>());
    let answer = asked(&live, "report", 1).unwrap();
    assert_eq!((answer.rows.len(), answer.more), (SHOWN, false), "nothing was left out, there is just no room");
}

#[test]
fn row_carries_place_kind_marks() {
    let answer = asked(&desk(), "cal", 1).unwrap();
    let row = &answer.rows[0];
    assert_eq!((&*row.name, &*row.place, row.kind, row.folder), ("Calendar", "/Applications", Kind::App, true));
    assert_eq!((lit(row), row.key), (vec![(0, 3)], key_of(Path::new("/Applications/Calendar.app")).0));
    assert_eq!((row.app, &*row.path), (true, Path::new("/Applications/Calendar.app")), "what opens is the bundle");
    assert!(row.why.is_none() && !row.predicted && row.icon.is_none());
    let notes = &asked(&desk(), "notes cargo", 1).unwrap().rows[0];
    assert_eq!((&*notes.place, lit(notes), notes.app), ("~/Documents", vec![(0, 5), (6, 11)], false));
}

#[test]
fn letters_spread_when_little_matches() {
    let live = from_paths(&["/Users/john/Documents/Cargo.toml", "/Users/john/cardigan.txt", "/Users/john/zebra.txt"]);
    let answer = asked(&live, "crgo", 1).unwrap();
    assert_eq!(answer.rows.iter().map(|row| &*row.name).collect::<Vec<_>>(), ["Cargo.toml"]);
    assert_eq!(lit(&answer.rows[0]), [(0, 1), (2, 5)]);
    assert_eq!(names(&live, "cg"), Vec::<String>::new(), "two letters are too few to guess from");
    assert_eq!(names(&live, "car"), ["Cargo.toml", "cardigan.txt"], "nothing else has those letters in order");
}

#[test]
fn spent_junk_budget_sets_more() {
    let budget = JUNK_BUDGET[2];
    let live = junk_drawer(budget + 500);
    let (context, latest) = (Context::at(EPOCH, 0, None), AtomicU64::new(0));
    let (query, stop) = (parse("cache", &context), Stop { latest: &latest, mine: 0 });
    let ground = Ground::new(&live, &query, &context, None, &stop).showing(true);
    let last = format!("/Users/john/Library/Caches/store/cache-{}.tmp", budget + 499);
    let id = live.resolve(Path::new(&last)).unwrap();
    let mut scratch = Scratch::default();
    let (scanned, more) = search(&ground, &mut scratch, &[], mark::allowance()).unwrap();
    assert!(more && scanned.iter().all(|found| found.id != id), "the last junk file is beyond the budget");
    let stored =
        [(key_of(Path::new(&last)), Box::from(last.as_str())), (key_of(Path::new("/gone")), Box::from("/gone"))];
    let (remembered, more) = search(&ground, &mut scratch, &stored, mark::allowance()).unwrap();
    assert!(more && remembered.iter().any(|found| found.id == id), "what was used before is outside every budget");
    assert_eq!(remembered.len(), scanned.len() + 1);
    let again = search(&ground, &mut scratch, &[], mark::allowance()).unwrap().0;
    assert!(!again.is_empty() && scratch.own.len() == live.len());
    let few = junk_drawer(budget);
    let ground = Ground::new(&few, &query, &context, None, &stop).showing(true);
    assert!(
        !search(&ground, &mut scratch, &[], mark::allowance()).unwrap().1,
        "a budget that was just enough left nothing out"
    );
}

#[test]
fn leading_junk_looked_at_first() {
    let budget = JUNK_BUDGET[2];
    let mut live = junk_drawer(0);
    let store = live.resolve(Path::new("/Users/john/Library/Caches/store")).unwrap();
    let cached = with_tier(0, 3);
    for nth in 0..budget + 10 {
        live.add(Some(store), Entry { name: &format!("old-index-{nth}"), modified: 1, created: 1, flags: cached });
    }
    let late = plant(
        &mut live,
        &Planted { path: "/Users/john/Library/Caches/store/index.db", modified: 1, created: 1, flags: cached },
    );
    assert_eq!(live.flags(late) & flag::TIER_MASK, cached);
    let answer = asked_for_everything(&live, "index").unwrap();
    assert_eq!((&*answer.rows[0].name, answer.more), ("index.db", true));
}

fn drawer_past_the_allowance() -> (Live, [EntryId; 4]) {
    let mut live = junk_drawer(2 * JUNK_BUDGET[2] + 500);
    let cached = with_tier(0, 3);
    let mut put = |path: &str| plant(&mut live, &Planted { path, modified: 1, created: 1, flags: cached });
    let shelf = put("/Users/john/Library/Caches/store/shelf/");
    let deep = put("/Users/john/Library/Caches/store/shelf/zz.dat");
    let quiet = put("/Users/john/Library/Caches/store/quiet.bin");
    let last = format!("/Users/john/Library/Caches/store/cache-{}.tmp", 2 * JUNK_BUDGET[2] + 499);
    let last = live.resolve(Path::new(&last)).expect("the last cache file");
    (live, [shelf, deep, quiet, last])
}

#[test]
fn word_beside_letter_matches_fully() {
    let (live, [shelf, deep, _, last]) = drawer_past_the_allowance();
    let (context, latest) = (Context::at(EPOCH, 0, None), AtomicU64::new(0));
    let stop = Stop { latest: &latest, mine: 0 };
    let found = |text: &str| {
        let query = parse(text, &context);
        let ground = Ground::new(&live, &query, &context, None, &stop).showing(true);
        assert_eq!(ground.late(shelf, ground.all()), 0, "every name was marked, nothing is read late");
        search(&ground, &mut Scratch::default(), &[], None).unwrap().0
    };
    let named = found(&format!("e {}", 2 * JUNK_BUDGET[2] + 499));
    assert_eq!(named.iter().map(|found| (found.id, found.fit.own)).collect::<Vec<_>>(), [(last, 0b11)]);
    let below = found("e zz");
    assert_eq!(below.iter().map(|found| (found.id, found.fit.above(0))).collect::<Vec<_>>(), [(deep, Some(shelf))]);
    let mut above: Vec<_> = found("shelf t").iter().map(|found| (found.id, found.fit.above(0))).collect();
    above.sort_unstable();
    assert_eq!(above, [(shelf, None), (deep, Some(shelf))], "the letter is only in the file's own name");
}

#[test]
fn lone_letters_read_late() {
    let (live, [shelf, deep, quiet, last]) = drawer_past_the_allowance();
    let (context, latest) = (Context::at(EPOCH, 0, None), AtomicU64::new(0));
    let stop = Stop { latest: &latest, mine: 0 };
    let query = parse("e q", &context);
    let ground = Ground::new(&live, &query, &context, None, &stop).showing(true);
    let found = |text: &str| {
        let query = parse(text, &context);
        search(&Ground::new(&live, &query, &context, None, &stop).showing(true), &mut Scratch::default(), &[], None)
            .unwrap()
    };
    let (alone, more) = found("e");
    let left_out = |id: EntryId| alone.iter().all(|found| found.id != id);
    assert!(more && left_out(last) && left_out(shelf) && left_out(quiet), "junk past the allowance is not marked");
    assert_eq!(found("e q").0.iter().map(|found| (found.id, found.fit.own)).collect::<Vec<_>>(), [(quiet, 0b11)]);
    let below = found("e z").0;
    assert_eq!(below.iter().map(|found| (found.id, found.fit.above(0))).collect::<Vec<_>>(), [(deep, Some(shelf))]);
    assert_eq!((ground.late(quiet, 0b01), ground.late(quiet, 0b10)), (0b01, 0b10 | START), "q starts the name");
    assert_eq!((ground.late(deep, 0b01), ground.late(quiet, 0)), (0, 0));
}
