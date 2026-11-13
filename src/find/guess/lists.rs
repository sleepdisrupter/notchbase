use std::path::{Path, PathBuf};

use super::golden::{CARGO, MAIN, MONDAY_NOON, TODO, at, edit, key, open, world};
use super::suggest::{Sight, Suggested, suggest, topped_up};
use crate::find::context::{Context, DAY, HOUR, MINUTE};
use crate::find::model::Live;
use crate::find::tests::corpus::from_paths;
use crate::find::usage::{Seen, Usage, Used};
use crate::find::view::{SHOWN, Why};

const README: &str = "/Users/john/GitHub/notchbase/README.md";

fn listed(usage: &Usage, live: &Live, context: &Context) -> Vec<(String, Why, bool)> {
    usage.read(|model| named(live, suggest(live, model, context, SHOWN)))
}

fn named(live: &Live, rows: Vec<Suggested>) -> Vec<(String, Why, bool)> {
    let mut name = String::new();
    let name_of = |row: &Suggested| {
        live.name_into(row.id, &mut name);
        name.clone()
    };
    rows.iter().map(name_of).zip(rows.iter()).map(|(name, row)| (name, row.why.clone(), row.predicted)).collect()
}

fn names(rows: &[(String, Why, bool)]) -> Vec<&str> {
    rows.iter().map(|(name, ..)| name.as_str()).collect()
}

fn seeded(usage: &Usage, path: &str, last: u32) {
    usage.seed(&[Used { path: PathBuf::from(path), last }], MONDAY_NOON);
}

fn taught_that_main_follows_cargo(usage: &Usage) -> Context {
    for round in 0..3 {
        open(usage, CARGO, "", &at(MONDAY_NOON + round * HOUR, Some("Xcode")));
        open(usage, MAIN, "", &at(MONDAY_NOON + round * HOUR + MINUTE, Some("Xcode")));
    }
    open(usage, CARGO, "", &at(MONDAY_NOON + 3 * HOUR, Some("Xcode")));
    at(MONDAY_NOON + 3 * HOUR + MINUTE, Some("Xcode"))
}

#[test]
fn usual_next_predicted_first() {
    let (live, usage) = (world(), Usage::load(None, MONDAY_NOON));
    let now = taught_that_main_follows_cargo(&usage);
    let rows = listed(&usage, &live, &now);
    assert_eq!(rows[0], ("main.rs".to_owned(), Why::After("Cargo.toml".into()), true));
    assert_eq!(rows[1], ("Cargo.toml".to_owned(), Why::Opened(MONDAY_NOON + 3 * HOUR), false));
    assert_eq!(rows.len(), 2, "each file once, and the file in hand is not a prediction");
    let much_later = at(now.now + DAY, Some("Xcode"));
    let cold = listed(&usage, &live, &much_later);
    assert!(
        cold.iter().all(|(name, why, predicted)| !matches!(why, Why::After(_)) && !(name == "main.rs" && *predicted))
    );
}

#[test]
fn four_skips_remove_prediction() {
    let (live, usage) = (world(), Usage::load(None, MONDAY_NOON));
    let now = taught_that_main_follows_cargo(&usage);
    let predicted = |context: &Context| {
        listed(&usage, &live, context).iter().any(|(name, _, predicted)| name == "main.rs" && *predicted)
    };
    for skip in 0..3 {
        usage.left(&[key(MAIN)], None, now.now);
        assert!(predicted(&now), "after skip {skip}");
    }
    usage.left(&[key(MAIN)], None, now.now);
    assert!(!predicted(&now));
    assert!(names(&listed(&usage, &live, &now)).contains(&"main.rs"), "it stays among the recents");
    let a_month_on = at(now.now + 30 * DAY, Some("Xcode"));
    open(&usage, CARGO, "", &at(a_month_on.now - MINUTE, Some("Xcode")));
    assert!(!predicted(&a_month_on), "time alone does not bring it back");
    open(&usage, MAIN, "", &a_month_on);
    open(&usage, CARGO, "", &at(a_month_on.now + MINUTE, Some("Xcode")));
    assert!(predicted(&at(a_month_on.now + 2 * MINUTE, Some("Xcode"))));
}

#[test]
fn prediction_needs_evidence() {
    let (live, usage) = (world(), Usage::load(None, MONDAY_NOON));
    let nothing_predicted = |context: &Context| listed(&usage, &live, context).iter().all(|(.., predicted)| !predicted);
    (0..3).for_each(|day| open(&usage, TODO, "", &at(MONDAY_NOON + day * DAY, Some("Preview"))));
    let thursday = MONDAY_NOON + 3 * DAY;
    assert!(nothing_predicted(&at(thursday, Some("Preview"))), "three opens are not yet a time of day");
    open(&usage, TODO, "", &at(thursday, Some("Preview")));
    let that_afternoon = thursday + 5 * HOUR;
    let with_the_app = listed(&usage, &live, &at(that_afternoon, Some("Preview")));
    assert_eq!(with_the_app[0], ("todo.md".to_owned(), Why::With("Preview".into()), true));
    assert!(nothing_predicted(&at(that_afternoon, Some("Mail"))), "the time of day alone is not enough");
    edit(&usage, "/Users/john/notes/minutes.md", 3, that_afternoon - 20 * MINUTE);
    let in_a_busy_folder = listed(&usage, &live, &at(that_afternoon, Some("Mail")));
    assert_eq!(in_a_busy_folder[0], ("todo.md".to_owned(), Why::AroundNow, true));
    assert!(nothing_predicted(&at(that_afternoon + 12 * HOUR, Some("Mail"))), "not in the small hours");
}

#[test]
fn edited_only_never_suggested() {
    let (live, usage) = (world(), Usage::load(None, MONDAY_NOON));
    seeded(&usage, README, MONDAY_NOON - HOUR);
    let now = at(edit(&usage, TODO, 6, MONDAY_NOON), None);
    let rows = listed(&usage, &live, &now);
    assert_eq!(names(&rows), ["README.md", "notes"]);
    assert_eq!(rows[1].1, Why::Busy(6));
    assert!(rows.iter().all(|(_, why, _)| !matches!(why, Why::Edited(_))));
    open(&usage, TODO, "", &now);
    assert_eq!(names(&listed(&usage, &live, &now)), ["todo.md", "README.md", "notes"]);
}

#[test]
fn recents_then_hot_folder_then_used() {
    let files: Vec<String> = (0..13).map(|serial| format!("/Users/john/work/file-{serial:02}.txt")).collect();
    let mut paths: Vec<&str> = files.iter().map(String::as_str).collect();
    paths.push("/Users/john/busy/draft.txt");
    let (live, usage) = (from_paths(&paths), Usage::load(None, MONDAY_NOON));
    for (serial, file) in files.iter().enumerate().skip(1) {
        seeded(&usage, file, MONDAY_NOON - serial as u32 * HOUR);
    }
    (0..6).for_each(|day| seeded(&usage, &files[0], MONDAY_NOON - 20 * DAY + day * DAY));
    let now = at(edit(&usage, "/Users/john/busy/draft.txt", 4, MONDAY_NOON), None);
    let rows = listed(&usage, &live, &now);
    let expected = [
        "file-01.txt",
        "file-02.txt",
        "file-03.txt",
        "busy",
        "file-04.txt",
        "file-05.txt",
        "file-06.txt",
        "file-07.txt",
        "file-08.txt",
        "file-09.txt",
        "file-10.txt",
        "file-00.txt",
        "file-11.txt",
        "file-12.txt",
    ];
    assert_eq!(names(&rows), expected);
    assert_eq!(rows[3].1, Why::Busy(4));
    assert_eq!(rows[0].1, Why::Opened(MONDAY_NOON - HOUR));
    let first_three = usage.read(|model| suggest(&live, model, &now, 3));
    assert_eq!(names(&named(&live, first_three)), ["file-01.txt", "file-02.txt", "file-03.txt"]);
    assert!(usage.read(|model| suggest(&live, model, &now, 0)).is_empty());
}

#[test]
fn missing_files_left_out() {
    let (live, usage) = (world(), Usage::load(None, MONDAY_NOON));
    seeded(&usage, "/Users/john/Documents/not-indexed.pdf", MONDAY_NOON - HOUR);
    seeded(&usage, README, MONDAY_NOON - 2 * HOUR);
    seeded(&usage, CARGO, MONDAY_NOON - 3 * HOUR);
    usage.observe(&[Seen::Removed { path: PathBuf::from(README) }], MONDAY_NOON);
    assert_eq!(names(&listed(&usage, &live, &at(MONDAY_NOON, None))), ["Cargo.toml"]);
    let mut live = live;
    let cargo = live.resolve(Path::new(CARGO)).unwrap();
    live.bury(cargo);
    live.settle();
    assert!(listed(&usage, &live, &at(MONDAY_NOON, None)).is_empty());
}

#[test]
fn newest_fill_when_little_learned() {
    let documents: Vec<String> = (0..8).map(|serial| format!("/Users/john/Documents/scan-{serial}.pdf")).collect();
    let mut paths: Vec<&str> = documents.iter().map(String::as_str).collect();
    paths.extend([CARGO, MAIN, TODO]);
    let (live, usage) = (from_paths(&paths), Usage::load(None, MONDAY_NOON));
    let pdfs = |name: &[u8], _flags: u8| name.ends_with(b".pdf");
    let home = Some(Path::new("/Users/john"));
    let now = at(MONDAY_NOON, None);
    let filled = |usage: &Usage| {
        usage.read(|model| named(&live, topped_up(&live, model, &now, (SHOWN, Sight::default()), home, &pdfs)))
    };
    let day_one = filled(&usage);
    assert_eq!(day_one.len(), 8);
    assert!(
        day_one
            .iter()
            .all(|(name, why, predicted)| name.ends_with(".pdf") && matches!(why, Why::Added(_)) && !predicted)
    );
    seeded(&usage, CARGO, MONDAY_NOON - HOUR);
    seeded(&usage, &documents[3], MONDAY_NOON - 2 * HOUR);
    let later = filled(&usage);
    assert_eq!(names(&later)[..2], ["Cargo.toml", "scan-3.pdf"]);
    assert_eq!(later.len(), 9, "the learned rows lead and no file is listed twice");
    for file in [MAIN, TODO, &documents[0], &documents[1]] {
        seeded(&usage, file, MONDAY_NOON - 3 * HOUR);
    }
    assert_eq!(filled(&usage).len(), 6, "a full screen of learned rows needs no filling");
}
