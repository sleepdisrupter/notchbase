use crate::find::context::HOUR;
use crate::find::model::{Live, with_tier};
use crate::find::tests::corpus::grown;

use super::golden::{LONG_AGO, NOW, asked, put};

const ENTRIES: usize = 200_000;
const SEED: u64 = 2_026;
const JUNK: usize = 500;

fn big() -> Live {
    let mut live = grown(SEED, ENTRIES);
    for package in 0..JUNK {
        let path = format!("/Users/john/Documents/GitHub/notchbase/node_modules/pkg-{package}/eslint-{package}.js");
        put(&mut live, &path, NOW - HOUR, with_tier(0, 2));
    }
    put(&mut live, "/Applications/Calendar.app/", LONG_AGO, 0);
    put(&mut live, "/Applications/Evernote.app/", NOW - HOUR, 0);
    put(&mut live, "/Users/john/Documents/GitHub/notchbase/Cargo.toml", NOW - 3 * HOUR, 0);
    put(&mut live, "/Users/john/Documents/essay.txt", NOW - HOUR, 0);
    put(&mut live, "/Users/john/Desktop/expenses.xlsx", NOW - HOUR, 0);
    put(&mut live, "/Users/john/work/2024/clients/acme/drafts/final/v2/edge-case.md", NOW - HOUR, 0);
    live
}

#[test]
fn big_index_keeps_user_files() {
    let live = big();
    assert!(live.len() > ENTRIES);
    let cargo = asked(&live, "ca");
    let top: Vec<(&str, &str)> = cargo.rows.iter().take(2).map(|row| (&*row.name, &*row.place)).collect();
    assert_eq!(top, [("Calendar", "/Applications"), ("Cargo.toml", "~/Documents/GitHub/notchbase")]);
    assert!(!cargo.more, "thousands of other cargo files are user files, and none was cut");

    let started = asked(&live, "e");
    let mut top: Vec<&str> = started.rows.iter().take(4).map(|row| &*row.name).collect();
    assert_eq!((top[0], started.rows[0].app), ("Evernote", true));
    top.sort_unstable();
    assert_eq!(
        top,
        ["Evernote", "edge-case.md", "essay.txt", "expenses.xlsx"],
        "every planted name that starts with e"
    );
    assert!(started.rows.iter().all(|row| !row.place.contains("node_modules")), "junk stays below user files");
}
