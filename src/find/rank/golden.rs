use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use crate::find::context::{Context, DAY, HOUR, Stamp};
use crate::find::model::{Live, flag, with_tier};
use crate::find::query::{Scratch, Stop, run};
use crate::find::shared::Ask;
use crate::find::tests::corpus::{Planted, plant};
use crate::find::usage::Usage;
use crate::find::view::{Answer, FindRow, SHOWN};

pub(super) const NOW: Stamp = 1_704_110_400 + 9 * DAY;
pub(super) const LONG_AGO: Stamp = NOW - 400 * DAY;

const HOME: &str = "/Users/john";
const BERLIN: i32 = 3600;

pub(super) fn asked(live: &Live, text: &str) -> Answer {
    answered(live, &Usage::load(None, 0), text)
}

pub(super) fn answered(live: &Live, usage: &Usage, text: &str) -> Answer {
    looked(live, usage, text, false)
}

pub(super) fn asked_for_everything(live: &Live, text: &str) -> Answer {
    looked(live, &Usage::load(None, 0), text, true)
}

fn looked(live: &Live, usage: &Usage, text: &str, everything: bool) -> Answer {
    let ask = Ask { generation: 1, text: Arc::from(text), context: Context::at(NOW, BERLIN, None) };
    let latest = AtomicU64::new(1);
    let mut scratch = Scratch::new(Some(PathBuf::from(HOME)));
    scratch.everything = everything;
    run(live, usage, &ask, &mut scratch, &Stop { latest: &latest, mine: 1 }).expect("nothing newer was asked")
}

pub(super) fn names(live: &Live, text: &str) -> Vec<String> {
    asked(live, text).rows.iter().map(|row| row.name.to_string()).collect()
}

pub(super) fn put(live: &mut Live, path: &str, changed: Stamp, flags: u8) {
    plant(live, &Planted { path, modified: changed, created: LONG_AGO.min(changed), flags });
}

pub(super) fn lit(row: &FindRow) -> Vec<(u16, u16)> {
    row.marks.iter().map(|lit| (lit.start, lit.end)).collect()
}

fn old(paths: &[&str]) -> Live {
    let mut live = Live::empty();
    for path in paths {
        put(&mut live, path, LONG_AGO, 0);
    }
    live
}

fn desk() -> Live {
    let mut live = old(&[
        "/Users/john/Documents/GitHub/notchbase/README.md",
        "/Users/john/Documents/GitHub/notchbase/src/main.rs",
        "/Users/john/Documents/GitHub/notchbase/src/lib.rs",
        "/Users/john/Documents/GitHub/other/README.md",
        "/Users/john/Documents/GitHub/other/src/",
        "/Users/john/Documents/notchbase-notes.txt",
        "/Users/john/Documents/pdf-notes.txt",
        "/Users/john/Documents/main.rst",
        "/Users/john/Projects/",
    ]);
    put(&mut live, "/Users/john/Documents/taxes.pdf", NOW - HOUR, 0);
    put(&mut live, "/Users/john/Documents/manual.pdf", NOW - 8 * DAY, 0);
    put(&mut live, "/Users/john/Documents/letter.pdf", NOW - DAY, 0);
    put(&mut live, "/Users/john/todo.txt", NOW - 2 * HOUR, 0);
    live
}

#[test]
fn checked_example_order() {
    let mut live = old(&["/Applications/Calendar.app/", "/Users/john/Documents/vacation.txt", "/Users/john/scan.pdf"]);
    put(&mut live, "/Users/john/Documents/GitHub/notchbase/Cargo.toml", NOW - 3 * HOUR, 0);
    put(&mut live, "/Users/john/.cargo/", LONG_AGO, flag::HIDDEN);
    let answer = asked_for_everything(&live, "ca");
    let order: Vec<&str> = answer.rows.iter().map(|row| &*row.name).collect();
    assert_eq!(order, ["Calendar", "Cargo.toml", ".cargo", "Applications", "scan.pdf", "vacation.txt"]);
    let apps: Vec<bool> = answer.rows.iter().map(|row| row.app).collect();
    assert_eq!(apps, [true, false, false, false, false, false], "the app is titled by its name alone");
    assert_eq!((&*answer.rows[1].place, lit(&answer.rows[1])), ("~/Documents/GitHub/notchbase", vec![(0, 2)]));
    assert_eq!((lit(&answer.rows[2]), answer.reading.clone(), answer.more), (vec![(1, 3)], None, false));
    let calm: Vec<String> = asked(&live, "ca").rows.iter().map(|row| row.name.to_string()).collect();
    assert_eq!(
        calm,
        ["Calendar", "Cargo.toml", "Applications", "scan.pdf", "vacation.txt"],
        "the hidden folder is left out"
    );
    assert_eq!(names(&live, ".ca"), [".cargo"], "typed with its dot, it is what was asked for");
}

#[test]
fn config_json_project_first() {
    let mut live = Live::empty();
    for package in 0..26 {
        let path = format!("/Users/john/Documents/GitHub/notchbase/node_modules/pkg-{package}/config.json");
        put(&mut live, &path, NOW - HOUR, with_tier(0, 2));
    }
    for app in 0..13 {
        let path = format!("/Users/john/Library/Application Support/App {app}/config.json");
        put(&mut live, &path, NOW - HOUR, with_tier(0, 1));
    }
    put(&mut live, "/Users/john/Documents/GitHub/notchbase/config.json", LONG_AGO, 0);
    let answer = asked_for_everything(&live, "config.json");
    let places: Vec<&str> = answer.rows.iter().map(|row| &*row.place).collect();
    assert_eq!((places.len(), places[0]), (SHOWN, "~/Documents/GitHub/notchbase"));
    assert!(places[1..14].iter().all(|place| place.starts_with("~/Library/Application Support/")));
    assert!(places[14..].iter().all(|place| place.contains("/node_modules/")));
    assert!(!answer.more, "forty matches are far inside every budget");
    let calm = asked(&live, "config.json");
    let places: Vec<&str> = calm.rows.iter().map(|row| &*row.place).collect();
    assert_eq!(places, ["~/Documents/GitHub/notchbase"], "only the file of the user's own is listed");
}

#[test]
fn tucked_away_listed_last() {
    let mut live = old(&["/Users/john/Documents/zsh-cheatsheet.md"]);
    put(&mut live, "/Users/john/.zshrc", NOW - HOUR, flag::HIDDEN);
    put(&mut live, "/Users/john/.config/", LONG_AGO, flag::HIDDEN);
    put(&mut live, "/Users/john/.config/zsh/aliases.zsh", NOW - HOUR, 0);
    put(&mut live, "/Users/john/Library/Preferences/com.apple.zshprefs.plist", NOW - HOUR, with_tier(0, 1));
    assert_eq!(names(&live, "zsh"), ["zsh-cheatsheet.md"], "one file of the user's own is the whole list");
    assert_eq!(names(&live, "zshrc"), [".zshrc"], "with nothing else to show, the hidden file is found");
    assert_eq!(names(&live, ".zsh"), [".zshrc"], "typed with its dot it is asked for, and only what is hidden");
    assert!(names(&live, "aliases").is_empty(), "what a tool keeps inside its hidden folder is not offered");
    assert!(names(&live, "zshprefs").is_empty(), "nor is the system's own, however little else there is");
    assert_eq!(names(&live, "zsh/aliases"), ["aliases.zsh"], "a path reaches wherever it points");
    assert_eq!(names(&live, "preferences/zsh"), ["com.apple.zshprefs.plist"]);
    let everything: Vec<String> =
        asked_for_everything(&live, "zsh").rows.iter().map(|row| row.name.to_string()).collect();
    assert_eq!(everything.len(), 5, "{everything:?}");
    let usage = Usage::load(None, 0);
    let pick = crate::find::view::Pick {
        path: "/Users/john/.zshrc".into(),
        typed: "zsh".to_owned(),
        rank: 1,
        how: crate::find::view::How::Open,
        predicted: false,
    };
    usage.picked(&pick, crate::find::kind::Kind::Other, &Context::at(NOW - HOUR, 0, None));
    let taught: Vec<String> = answered(&live, &usage, "zsh").rows.iter().map(|row| row.name.to_string()).collect();
    assert_eq!(taught, [".zshrc", "zsh-cheatsheet.md"], "picked once for these letters, it is the user's own");
}

#[test]
fn both_word_orders_find_readme() {
    for text in ["notchbase readme", "readme notchbase", "README  NotchBase"] {
        let answer = asked(&desk(), text);
        let rows: Vec<(&str, &str)> = answer.rows.iter().map(|row| (&*row.name, &*row.place)).collect();
        assert_eq!(rows, [("README.md", "~/Documents/GitHub/notchbase")], "{text}");
        assert_eq!(lit(&answer.rows[0]), [(0, 6)], "only the word found in the name is lit");
    }
    assert_eq!(
        names(&desk(), "notchbase"),
        ["notchbase", "notchbase-notes.txt"],
        "a folder word alone finds no file below it"
    );
}

#[test]
fn readings_alone_return_rows() {
    let today = asked(&desk(), "today");
    assert_eq!(today.rows.iter().map(|row| &*row.name).collect::<Vec<_>>(), ["taxes.pdf", "todo.txt"]);
    assert_eq!(today.reading.as_deref(), Some("Today"));
    let folders = asked(&desk(), "folders");
    assert!(folders.rows.len() >= 8 && folders.rows.iter().all(|row| row.folder), "{:?}", names(&desk(), "folders"));
    assert!(folders.rows.iter().any(|row| &*row.name == "Projects") && folders.reading.as_deref() == Some("Folders"));
    let both = asked(&desk(), "pdf today");
    assert_eq!(both.rows.iter().map(|row| &*row.name).collect::<Vec<_>>(), ["taxes.pdf"]);
    assert_eq!(both.reading.as_deref(), Some("PDFs \u{b7} today"));
}

#[test]
fn kind_word_read_both_ways() {
    assert_eq!(names(&desk(), "pdf"), ["pdf-notes.txt", "taxes.pdf", "letter.pdf", "manual.pdf"]);
    assert_eq!(names(&desk(), "today"), ["taxes.pdf", "todo.txt"]);
    let mut live = desk();
    put(&mut live, "/Users/john/today-plan.md", LONG_AGO, 0);
    assert_eq!(names(&live, "today"), ["today-plan.md", "taxes.pdf", "todo.txt"], "the name and the date both count");
    put(&mut live, "/Users/john/Pictures/folder.jpg", LONG_AGO, 0);
    assert_eq!(names(&live, "folder")[0], "folder.jpg");
    assert_eq!(names(&live, "image"), ["folder.jpg"]);
}

#[test]
fn endings_and_paths_narrow() {
    let live = desk();
    assert_eq!(names(&live, "*.rs"), ["lib.rs", "main.rs"]);
    assert_eq!(names(&live, ".rs"), ["lib.rs", "main.rs", "main.rst"], "a bare ending is also plain letters");
    assert_eq!(names(&live, "main *.rs"), ["main.rs"]);
    assert_eq!(names(&live, "src/"), ["src", "src"]);
    assert_eq!(names(&live, "src/main"), ["main.rs"]);
    assert_eq!(names(&live, "notch/src/*.rs"), ["lib.rs", "main.rs"]);
    assert_eq!(names(&live, "gh/notch"), ["notchbase"], "GitHub by its initials");
    assert_eq!(names(&live, "~/doc"), ["Documents"]);
    assert_eq!(names(&live, "other/readme"), ["README.md"]);
    assert_eq!(asked(&live, "other/readme").rows[0].place.as_ref(), "~/Documents/GitHub/other");
    assert_eq!(names(&live, "nowhere/readme"), Vec::<String>::new());
}

#[test]
fn dates_select_by_local_day() {
    let live = desk();
    assert_eq!(names(&live, "yesterday"), ["letter.pdf"]);
    assert_eq!(names(&live, "this week"), ["taxes.pdf", "todo.txt", "letter.pdf"], "it is Wednesday");
    assert_eq!(names(&live, "last week"), ["manual.pdf"]);
    assert_eq!(names(&live, "this month"), ["taxes.pdf", "todo.txt", "letter.pdf", "manual.pdf"]);
    assert_eq!(names(&live, "recent pdf"), ["taxes.pdf", "letter.pdf"]);
    assert_eq!(asked(&live, "pdf last week").reading.as_deref(), Some("PDFs \u{b7} last week"));
}

#[test]
fn capitals_prefer_capitals() {
    let live = old(&["/Users/john/a/readme.md", "/Users/john/b/README.md", "/Users/john/c/Readme.md"]);
    assert_eq!(asked(&live, "README").rows[0].place.as_ref(), "~/b");
    assert_eq!(asked(&live, "Readme").rows[0].place.as_ref(), "~/c");
    assert_eq!(asked(&live, "readme").rows.len(), 3);
}

#[test]
#[ignore = "needs the accent fold of the real index model; run once the tracks are merged"]
fn resume_finds_accented_name() {
    let live = old(&["/Users/john/r\u{e9}sum\u{e9}.pdf", "/Users/john/resume.pdf"]);
    let answer = asked(&live, "resume");
    let found: Vec<&str> = answer.rows.iter().map(|row| &*row.name).collect();
    assert_eq!(found, ["resume.pdf", "r\u{e9}sum\u{e9}.pdf"]);
    assert_ne!(answer.rows[0].key, answer.rows[1].key);
    assert_eq!(lit(&answer.rows[1]), [(0, 6)], "the accented letters are lit as letters");
}

#[test]
fn deep_user_file_survives_junk() {
    let mut live = old(&["/Users/john/Library/a/b/c/d/e/"]);
    let junk = live.resolve(std::path::Path::new("/Users/john/Library/a/b/c/d/e")).unwrap();
    let library = with_tier(0, 1);
    for nth in 0..20_000 {
        let entry =
            crate::find::model::Entry { name: &format!("index-{nth}.js"), modified: NOW, created: NOW, flags: library };
        live.add(Some(junk), entry);
    }
    put(&mut live, "/Users/john/p/a/b/c/d/e/f/index.html", LONG_AGO, 0);
    let answer = asked_for_everything(&live, "index");
    assert_eq!((&*answer.rows[0].name, &*answer.rows[0].place), ("index.html", "~/p/a/b/c/d/e/f"));
    assert!(answer.more, "the library's budget ran out");
    assert_eq!(answer.rows.len(), SHOWN);
    let calm: Vec<String> = asked(&live, "index").rows.iter().map(|row| row.name.to_string()).collect();
    assert_eq!(calm, ["index.html"], "and without the library there is nothing to wade through");
}

#[test]
fn named_before_kind() {
    let mut live = old(&["/Users/john/Projects/barcode-notes.txt", "/Users/john/Projects/decoder.txt"]);
    for fresh in ["server.js", "mod.rs", "macos.rs"] {
        put(&mut live, &format!("/Users/john/Projects/{fresh}"), NOW - HOUR, 0);
    }
    let found = names(&live, "code");
    assert_eq!(found[..2], ["decoder.txt", "barcode-notes.txt"], "however fresh the code files are: {found:?}");
    assert_eq!(found.len(), 5, "and the code files still follow");
    assert_eq!(names(&live, "code js"), ["server.js"], "beside another word the kind only narrows");
}

#[test]
fn three_rows_per_name_first() {
    let mut live = old(&["/Users/john/Projects/reader/", "/Users/john/Projects/read-later.txt"]);
    for (nth, folder) in ["app", "sdk", "tools", "setup", "wiki"].into_iter().enumerate() {
        put(&mut live, &format!("/Users/john/Projects/{folder}/README.md"), NOW - nth as Stamp * HOUR, 0);
    }
    let found = names(&live, "read");
    assert_eq!(found.len(), 7, "nothing is left out: {found:?}");
    assert_eq!(found[..3], ["README.md", "README.md", "README.md"]);
    assert_eq!(found[5..], ["README.md", "README.md"], "the fourth and fifth wait behind the other names");
    let only = names(&live, "readme");
    assert_eq!(only, ["README.md"; 5], "alone, they are simply listed");
}
