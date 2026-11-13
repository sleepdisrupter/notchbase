use std::path::Path;

use crate::find::context::{Context, HOUR, MINUTE, Stamp};
use crate::find::kind::Kind;
use crate::find::model::{Entry, Live, flag, with_tier};
use crate::find::tests::corpus::{Planted, plant};
use crate::find::usage::Usage;
use crate::find::view::{Answer, How, Pick, SHOWN};
use crate::find::weights::SHORTLIST;

use super::golden::{LONG_AGO, NOW, answered, asked, lit, put};

const SYSTEM: u8 = 1;
const SCREENSHOTS: Stamp = 25;
const JUNK_HITS: usize = 20_500;
const EVERYONE: &str = "/ProgramData";
const ROAMING: &str = "/Users/john/AppData";
const MENU: &str = "Microsoft/Windows/Start Menu/Programs";
const SHORTCUT: &str = "/ProgramData/Microsoft/Windows/Start Menu/Programs/TreeSize Free/TreeSize Free.lnk";
const BUNDLE: &str = "/Applications/TreeSize Free.app";
const SPACINGS: [&str; 6] =
    ["treesize free", "TreeSize  Free ", " treesize free", "tree size free", "treesizefree", "treesize"];

fn below(live: &mut Live, system: &str, rest: &str, changed: Stamp) {
    let junk = with_tier(0, SYSTEM);
    let path = format!("{system}/{rest}");
    for (end, _) in rest.match_indices('/') {
        put(live, &path[..=system.len() + 1 + end], LONG_AGO, junk);
    }
    put(live, &path, changed, junk);
}

fn screenshots(live: &mut Live) {
    for shot in 0..SCREENSHOTS {
        let path = format!("/Users/john/Desktop/Screenshots/shot-{shot:04}TreeSize_Free.png");
        let taken = NOW - shot * MINUTE;
        plant(live, &Planted { path: &path, modified: taken, created: taken, flags: 0 });
    }
}

fn the_testers_disk() -> Live {
    let mut live = Live::empty();
    below(&mut live, EVERYONE, &format!("{MENU}/TreeSize Free/TreeSize Free.lnk"), LONG_AGO);
    below(&mut live, "/Program Files", "JAM Software/TreeSize Free/TreeSizeFree.exe", LONG_AGO);
    put(&mut live, "/SOFTWARES/Executables/TreeSizeFreeSetup.exe", LONG_AGO, 0);
    screenshots(&mut live);
    live
}

fn its_mac_twin() -> Live {
    let mut live = Live::empty();
    let packaged = with_tier(flag::INSIDE_PACKAGE, SYSTEM);
    put(&mut live, "/Applications/TreeSize Free.app/", LONG_AGO, 0);
    put(&mut live, "/Applications/TreeSize Free.app/Contents/", LONG_AGO, packaged);
    put(&mut live, "/Applications/TreeSize Free.app/Contents/MacOS/", LONG_AGO, packaged);
    put(&mut live, "/Applications/TreeSize Free.app/Contents/MacOS/TreeSize Free", LONG_AGO, packaged);
    put(&mut live, "/Applications/Safari.app", LONG_AGO, flag::LINK);
    put(&mut live, "/Users/john/Downloads/TreeSizeFreeSetup.dmg", LONG_AGO, 0);
    put(&mut live, "/Users/john/Documents/safari-notes.txt", NOW - HOUR, 0);
    screenshots(&mut live);
    live
}

fn browsers() -> Live {
    let mut live = Live::empty();
    below(&mut live, EVERYONE, &format!("{MENU}/Google Chrome.lnk"), LONG_AGO);
    below(&mut live, EVERYONE, &format!("{MENU}/Visual Studio Code/Visual Studio Code.lnk"), LONG_AGO);
    put(&mut live, "/Users/john/Documents/chrome-notes.txt", LONG_AGO, 0);
    put(&mut live, "/Users/john/Documents/site/.vscode/", LONG_AGO, flag::HIDDEN);
    live
}

fn paths(answer: &Answer) -> Vec<String> {
    answer.rows.iter().map(|row| row.path.to_string_lossy().into_owned()).collect()
}

fn first(live: &Live, text: &str) -> (String, Kind, Vec<(u16, u16)>) {
    let answer = asked(live, text);
    assert_eq!(&*answer.query, text, "the answer names the typed text byte for byte");
    let row = answer.rows.first().unwrap_or_else(|| panic!("{text} finds something"));
    (row.path.to_string_lossy().into_owned(), row.kind, lit(row))
}

fn titled(live: &Live, text: &str) -> (String, bool) {
    let answer = asked(live, text);
    let row = answer.rows.first().unwrap_or_else(|| panic!("{text} finds something"));
    (row.name.to_string(), row.app)
}

#[test]
fn testers_app_leads_however_spaced() {
    let shown = ("TreeSize Free".to_owned(), true);
    for text in SPACINGS {
        let (path, kind, _) = first(&the_testers_disk(), text);
        assert_eq!((path.as_str(), kind), (SHORTCUT, Kind::App), "{text} on Windows");
        assert_eq!(titled(&the_testers_disk(), text), shown, "{text} on Windows");
        let (path, kind, _) = first(&its_mac_twin(), text);
        assert_eq!((path.as_str(), kind), (BUNDLE, Kind::App), "{text} on a Mac");
        assert_eq!(titled(&its_mac_twin(), text), shown, "{text} on a Mac");
    }
    let whole = vec![(0, 8), (9, 13)];
    assert_eq!(first(&the_testers_disk(), "treesizefree").2, whole, "the two words light up, the space stays dim");
    assert_eq!(first(&the_testers_disk(), "treesize free").2, whole);
    assert_eq!(first(&its_mac_twin(), "tree size free").2, whole);
}

#[test]
fn linked_app_is_an_app() {
    let (path, kind, lit) = first(&its_mac_twin(), "safari");
    assert_eq!((path.as_str(), kind, lit), ("/Applications/Safari.app", Kind::App, vec![(0, 6)]));
    assert_eq!(titled(&its_mac_twin(), "safari"), ("Safari".to_owned(), true));
    let started = paths(&asked(&its_mac_twin(), "saf"));
    assert_eq!(started, ["/Applications/Safari.app", "/Users/john/Documents/safari-notes.txt"], "the app leads");
}

#[test]
fn app_answers_words_and_initials() {
    let live = browsers();
    let chrome = format!("{EVERYONE}/{MENU}/Google Chrome.lnk");
    let code = format!("{EVERYONE}/{MENU}/Visual Studio Code/Visual Studio Code.lnk");
    for text in ["googlechrome", "google chrome", "Google-Chrome", "goog lechrome"] {
        assert_eq!(first(&live, text), (chrome.clone(), Kind::App, vec![(0, 6), (7, 13)]), "{text}");
    }
    let named = paths(&asked(&live, "chrome"));
    assert_eq!(named, [chrome.as_str(), "/Users/john/Documents/chrome-notes.txt"], "a word of an app is a start");
    assert_eq!(first(&live, "vsc"), (code.clone(), Kind::App, vec![(0, 1), (7, 8), (14, 15)]));
    assert_eq!(titled(&live, "vsc"), ("Visual Studio Code".to_owned(), true));
    assert_eq!(titled(&live, "googlechrome"), ("Google Chrome".to_owned(), true));
    assert_eq!(first(&live, "vs code").0, code);
    assert_eq!(first(&live, "v").0, code, "one letter is read like any other word");
}

#[test]
fn separator_optional() {
    let mut live = Live::empty();
    for name in ["my-file.txt", "my_file.txt", "My File.txt", "myfile.txt", "family.txt"] {
        put(&mut live, &format!("/Users/john/Documents/{name}"), LONG_AGO, 0);
    }
    let found =
        |text: &str| -> Vec<String> { asked(&live, text).rows.iter().map(|row| row.name.to_string()).collect() };
    let sorted = |mut names: Vec<String>| {
        names.sort_unstable();
        names
    };
    let all = ["My File.txt", "my-file.txt", "my_file.txt", "myfile.txt"];
    for text in ["my-file", "myfile", "my file", "my_file", "MY  FILE"] {
        assert_eq!(sorted(found(text)), all, "{text}");
    }
    assert_eq!(found("my-file")[0], "my-file.txt", "what was typed letter for letter comes first");
    assert_eq!(found("myfile")[0], "myfile.txt");
    assert_eq!(found("my_file")[0], "my_file.txt");
    let squeezed = asked(&live, "myfile");
    let spaced = squeezed.rows.iter().find(|row| &*row.name == "My File.txt").expect("found above");
    assert_eq!(lit(spaced), [(0, 2), (3, 7)]);
}

#[test]
fn two_word_pick_recalled() {
    let mut live = Live::empty();
    for name in ["My File.txt", "my-file-old.txt"] {
        put(&mut live, &format!("/Users/john/Documents/{name}"), LONG_AGO, 0);
    }
    let usage = Usage::load(None, 0);
    let first = |text: &str| answered(&live, &usage, text).rows[0].name.to_string();
    assert_eq!(first("myfile"), "My File.txt", "with nothing learned the whole name leads");
    let open_twice = |name: &str, typed: &str| {
        let path = format!("/Users/john/Documents/{name}");
        let pick = Pick { path: path.into(), typed: typed.to_owned(), rank: 0, how: How::Open, predicted: false };
        (0..2).for_each(|_| usage.picked(&pick, Kind::Text, &Context::at(NOW - HOUR, 0, None)));
    };
    open_twice("My File.txt", "txt");
    open_twice("my-file-old.txt", "my file");
    assert_eq!(first("txt"), "My File.txt", "both were opened as often, so what was typed for each decides");
    for text in ["my file", "my  file ", "myfile", "my-file", " MY_FILE"] {
        assert_eq!(first(text), "my-file-old.txt", "{text}");
    }
}

#[test]
fn named_app_before_fresh_file() {
    let mut mac = Live::empty();
    put(&mut mac, "/Applications/Calendar.app/", LONG_AGO, 0);
    let mut windows = Live::empty();
    below(&mut windows, EVERYONE, &format!("{MENU}/Calendar.lnk"), LONG_AGO);
    let cargo = "/Users/john/Documents/GitHub/notchbase/Cargo.toml";
    for (mut live, app) in [(mac, "/Applications/Calendar.app"), (windows, "/ProgramData/Microsoft/Windows")] {
        put(&mut live, cargo, NOW - 3 * HOUR, 0);
        put(&mut live, "/Users/john/Documents/calc-notes.txt", LONG_AGO, 0);
        let order = paths(&asked(&live, "ca"));
        assert!(order[0].starts_with(app), "{order:?}");
        assert_eq!(order[1..3], [cargo, "/Users/john/Documents/calc-notes.txt"]);
        let usage = Usage::load(None, 0);
        let pick = Pick { path: cargo.into(), typed: "ca".to_owned(), rank: 1, how: How::Open, predicted: false };
        usage.picked(&pick, Kind::Code, &Context::at(NOW - HOUR, 0, None));
        let taught = paths(&answered(&live, &usage, "ca"));
        assert_eq!(taught[0], cargo, "what was picked for these letters leads from then on");
        assert!(taught[1].starts_with(app), "{taught:?}");
        let longer = paths(&answered(&live, &usage, "cal"));
        assert!(longer[0].starts_with(app), "the file no longer matches, the app still does: {longer:?}");
    }
}

#[test]
fn loose_app_after_leading_file() {
    let mut live = Live::empty();
    put(&mut live, "/Applications/Evernote.app/", LONG_AGO, 0);
    put(&mut live, "/Applications/Spotify.app/", LONG_AGO, 0);
    put(&mut live, "/Users/john/Documents/note-ideas.md", LONG_AGO, 0);
    put(&mut live, "/Users/john/Documents/spot-prices.csv", LONG_AGO, 0);
    assert_eq!(
        paths(&asked(&live, "note")),
        ["/Users/john/Documents/note-ideas.md", "/Applications/Evernote.app"],
        "the inside of a word does not lead"
    );
    assert_eq!(paths(&asked(&live, "spot")), ["/Applications/Spotify.app", "/Users/john/Documents/spot-prices.csv"]);
    let slipped = asked(&live, "sptoify");
    assert_eq!(paths(&slipped), ["/Applications/Spotify.app"], "one slip still finds the app");
    assert!(slipped.rows[0].app && slipped.rows[0].marks.is_empty());
    let skipped = asked(&live, "sptfy");
    assert_eq!(paths(&skipped), ["/Applications/Spotify.app"], "letters left out still find it");
    assert_eq!(lit(&skipped.rows[0]), [(0, 2), (3, 4), (5, 7)], "and what was typed lights up");
}

#[test]
fn app_guess_fills_short_list() {
    let mut live = Live::empty();
    put(&mut live, "/Applications/Notion.app/", LONG_AGO, 0);
    for nth in 1..8 {
        put(&mut live, &format!("/Users/john/Documents/note-{nth}.md"), LONG_AGO, 0);
    }
    let guessed = asked(&live, "note");
    assert_eq!(guessed.rows.len(), 8, "seven notes, and one slip away from them an app");
    assert_eq!(guessed.rows.last().map(|row| (&*row.name, row.app)), Some(("Notion", true)), "the guess comes last");
    put(&mut live, "/Users/john/Documents/note-8.md", LONG_AGO, 0);
    let full = asked(&live, "note");
    assert_eq!(full.rows.len(), 8, "eight real answers need no guess beside them");
    assert!(full.rows.iter().all(|row| !row.app));
}

#[test]
fn launcher_survives_junk() {
    let mut live = Live::empty();
    for nth in 0..SHORTLIST + SHOWN {
        put(&mut live, &format!("/Users/john/Documents/treesize free notes {nth:03}.txt"), NOW - HOUR, 0);
    }
    put(&mut live, "/Users/john/Library/Caches/store/", LONG_AGO, 0);
    let store = live.resolve(Path::new("/Users/john/Library/Caches/store")).unwrap();
    let junk = with_tier(0, SYSTEM);
    for nth in 0..JUNK_HITS {
        let name = format!("treesize-free-{nth}.js");
        live.add(Some(store), Entry { name: &name, modified: NOW, created: NOW, flags: junk });
    }
    below(&mut live, EVERYONE, &format!("{MENU}/TreeSize Free/TreeSize Free.lnk"), LONG_AGO);
    let answer = asked(&live, "treesize free");
    assert_eq!((&*answer.rows[0].path, answer.rows[0].kind), (Path::new(SHORTCUT), Kind::App));
    assert_eq!(answer.rows.len(), SHOWN);
    assert!(answer.rows[1..].iter().all(|row| row.place.starts_with("~/Documents")), "then the user's own files");
}

#[test]
fn twin_shortcuts_are_one_row() {
    let mut live = the_testers_disk();
    let own = format!("Roaming/{MENU}/TreeSize Free.lnk");
    below(&mut live, ROAMING, &own, NOW - HOUR);
    for text in ["treesize free", "treesize", "treesizefree", "apps"] {
        let answer = asked(&live, text);
        let apps: Vec<&str> = answer.rows.iter().filter(|row| row.app).map(|row| &*row.name).collect();
        assert_eq!(apps, ["TreeSize Free"], "{text}");
    }
    assert_eq!(first(&live, "treesize free").0, format!("{ROAMING}/{own}"), "the one changed this hour");
    let usage = Usage::load(None, 0);
    let pick = Pick { path: SHORTCUT.into(), typed: "tree".to_owned(), rank: 0, how: How::Open, predicted: false };
    usage.picked(&pick, Kind::App, &Context::at(NOW - HOUR, 0, None));
    let learned = answered(&live, &usage, "tree");
    let shortcuts: Vec<String> =
        paths(&learned).into_iter().filter(|path| path.ends_with("TreeSize Free.lnk")).collect();
    assert_eq!((learned.rows[0].kind, shortcuts), (Kind::App, vec![SHORTCUT.to_owned()]), "the one picked before");
}

#[test]
fn apps_word_lists_launchers() {
    let disks = [(the_testers_disk(), SHORTCUT, SHORTCUT), (its_mac_twin(), "/Applications/Safari.app", BUNDLE)];
    for (live, app, tree) in disks {
        for text in ["apps", "applications", "app"] {
            let answer = asked(&live, text);
            assert_eq!(answer.reading.as_deref(), Some("Apps"), "{text}");
            let listed = answer.rows.iter().find(|row| row.path.to_string_lossy() == app);
            assert_eq!(listed.map(|row| (row.kind, row.app)), Some((Kind::App, true)), "{text} lists {app}");
        }
        assert_eq!(first(&live, "apps tree").0, tree, "a word beside the kind narrows it");
    }
}
