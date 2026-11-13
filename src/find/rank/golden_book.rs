use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use crate::find::apps::{Book, Card, Read, Used};
use crate::find::context::{Context, DAY};
use crate::find::key::key_of;
use crate::find::model::{Live, flag, with_tier};
use crate::find::query::{Scratch, Stop, run};
use crate::find::shared::Ask;
use crate::find::usage::Usage;
use crate::find::view::Answer;

use super::golden::{LONG_AGO, NOW, lit, put};

const SYSTEM: u8 = 1;
const WEATHER: &str =
    "/Users/john/Applications (Parallels)/{cd127fd0} Applications.localized/0097da900b02a0cd942cd60dd7b4850a.app";
const HELPER: &str = "/Applications/Utilities/Adobe Creative Cloud Experience/CCXProcess/CCXProcess.app";
const INSTRUMENTS: &str = "/Applications/Xcode.app/Contents/Applications/Instruments.app";
const KEYNOTE: &str = "/Applications/Keynote Creator Studio.app";
const FIND_MY: &str = "/System/Applications/FindMy.app";
const FINDER: &str = "/System/Library/CoreServices/Finder.app";
const PANES: &str = "/System/Library/ExtensionKit/Extensions";
const VM: &str = "/Users/john/Parallels/Windows 11.pvm/VM.app";
const UPDATER: &str = "/Users/john/Library/Application Support/Updaters/Ghost Updater.app";
const SITE: &str = "/Users/john/Library/Sites/https+++bettercmdtab.app";
const UNREAD: &str = "/Users/john/Library/Application Support/Unread Tool.app";
const NOTEPAD: &str =
    "/Users/john/Applications (Parallels)/{cd127fd0} Applications.localized/1ffcf5f3fb0fd82f75dceae06845d9e6.app";

fn this_mac() -> Live {
    let mut live = Live::empty();
    let bundles = [
        "/Applications/Zen.app/",
        "/Applications/Spotify.app/",
        "/Applications/Xcode.app/",
        "/System/Applications/Stocks.app/",
        "/System/Applications/Siri.app/",
        "/System/Applications/Siri AI.app/",
        "/System/Applications/Stickies.app/",
    ];
    let named = [KEYNOTE, FIND_MY, FINDER, WEATHER, NOTEPAD, HELPER].map(|app| format!("{app}/"));
    for bundle in bundles.into_iter().map(str::to_owned).chain(named) {
        put(&mut live, &bundle, LONG_AGO, 0);
    }
    put(&mut live, "/Applications/Safari.app", LONG_AGO, flag::LINK);
    for pane in ["Wi-Fi", "Bluetooth", "DisplaysExt", "AboutSettingsIntents", "NeverRead"] {
        put(&mut live, &format!("{PANES}/{pane}.appex/"), LONG_AGO, 0);
    }
    let packaged = with_tier(flag::INSIDE_PACKAGE, SYSTEM);
    put(&mut live, &format!("{VM}/"), LONG_AGO, packaged);
    for elsewhere in [UPDATER, SITE, UNREAD] {
        put(&mut live, &format!("{elsewhere}/"), LONG_AGO, with_tier(0, SYSTEM));
    }
    put(&mut live, "/Applications/Xcode.app/Contents/", LONG_AGO, packaged);
    put(&mut live, "/Applications/Xcode.app/Contents/Applications/", LONG_AGO, packaged);
    put(&mut live, &format!("{INSTRUMENTS}/"), LONG_AGO, packaged);
    let files = [
        "weather-report.pdf",
        "find-my-keys.txt",
        "creative-brief.txt",
        "siri-shortcuts.md",
        "sp-notes.txt",
        "notes.md",
    ];
    for file in files {
        put(&mut live, &format!("/Users/john/Documents/{file}"), NOW - DAY, 0);
    }
    live
}

fn card(path: &str, title: &str, backstage: bool, used: Used) -> (crate::find::key::Key, Card) {
    (
        key_of(Path::new(path)),
        Card::of(Read { title: Some(title.to_owned()), backstage, unlisted: false }, used, LONG_AGO),
    )
}

fn its_book() -> Book {
    let never = Used::default();
    Book::holding([
        card(WEATHER, "Weather", false, never),
        card(VM, "Windows 11", true, Used { last: NOW - 9 * DAY, times: 2 }),
        card(UPDATER, "Ghost Updater", true, never),
        (key_of(Path::new(SITE)), Card::of(Read { unlisted: true, ..Read::default() }, never, LONG_AGO)),
        card(&format!("{PANES}/Wi-Fi.appex"), "Wi-Fi", false, never),
        card(&format!("{PANES}/Bluetooth.appex"), "Bluetooth", false, never),
        card(&format!("{PANES}/DisplaysExt.appex"), "Displays", false, never),
        (
            key_of(Path::new(&format!("{PANES}/AboutSettingsIntents.appex"))),
            Card::of(Read { unlisted: true, ..Read::default() }, never, LONG_AGO),
        ),
        card(NOTEPAD, "Notepad", false, Used { last: NOW - 2 * DAY, times: 3 }),
        card(FIND_MY, "Find My", false, never),
        card(KEYNOTE, "Keynote", false, never),
        card(HELPER, "Creative Cloud Content Manager", true, never),
        card("/System/Applications/Siri.app", "Siri", false, never),
        card("/System/Applications/Siri AI.app", "Siri", false, never),
        card("/Applications/Zen.app", "Zen", false, Used { last: NOW, times: 1_736 }),
        card("/Applications/Spotify.app", "Spotify", false, Used { last: NOW - DAY, times: 66 }),
        card("/System/Applications/Stocks.app", "Stocks", false, Used { last: NOW - 3 * DAY, times: 12 }),
        card("/Applications/Safari.app", "Safari", false, never),
    ])
}

fn asked(text: &str) -> Answer {
    asked_with(text, &[])
}

fn asked_with(text: &str, open: &[&str]) -> Answer {
    asked_on(&this_mac(), text, open)
}

fn asked_on(live: &Live, text: &str, open: &[&str]) -> Answer {
    let running = open.iter().map(|app| key_of(Path::new(app))).collect();
    let ask = Ask { generation: 1, text: Arc::from(text), context: Context::at(NOW, 0, None).among(running) };
    let latest = AtomicU64::new(1);
    let mut scratch = Scratch::new(Some(PathBuf::from("/Users/john")));
    scratch.book = Arc::new(its_book());
    let usage = Usage::load(None, 0);
    run(live, &usage, &ask, &mut scratch, &Stop { latest: &latest, mine: 1 }).expect("nothing newer was asked")
}

fn titles(text: &str) -> Vec<String> {
    asked(text).rows.iter().map(|row| row.name.to_string()).collect()
}

fn first(text: &str) -> (String, String, bool, Vec<(u16, u16)>) {
    let answer = asked(text);
    let row = answer.rows.first().unwrap_or_else(|| panic!("{text} finds something"));
    (row.name.to_string(), row.path.to_string_lossy().into_owned(), row.app, lit(row))
}

#[test]
fn app_answers_to_shown_name() {
    assert_eq!(first("weather"), ("Weather".into(), WEATHER.into(), true, vec![(0, 7)]), "not to a hash");
    assert_eq!(first("find my"), ("Find My".into(), FIND_MY.into(), true, vec![(0, 4), (5, 7)]));
    assert_eq!(first("findmy"), ("Find My".into(), FIND_MY.into(), true, vec![(0, 4), (5, 7)]));
    assert_eq!(first("keynote"), ("Keynote".into(), KEYNOTE.into(), true, vec![(0, 7)]));
    assert_eq!(first("key").0, "Keynote");
}

#[test]
fn file_name_still_finds_app() {
    assert_eq!(first("creator studio"), ("Keynote".into(), KEYNOTE.into(), true, Vec::new()));
    assert_eq!(first("0097da"), ("Weather".into(), WEATHER.into(), true, Vec::new()));
    assert!(titles("2a0cd").is_empty(), "the inside of a file name nobody sees finds nothing");
    assert_eq!(titles("eynot"), ["Keynote"], "the inside of the shown name does");
}

#[test]
fn used_apps_come_first() {
    let started = titles("s");
    assert_eq!(started[..3], ["Spotify", "Stocks", "Siri"], "daily, now and then, never: {started:?}");
    let place = |title: &str| started.iter().position(|name| name == title).unwrap_or_else(|| panic!("{title}"));
    assert!(place("Siri") < place("Safari"), "with no use to go by, the shorter name is the closer one");
    assert!(place("Stickies") < place("sp-notes.txt"), "and every app still leads the files");
    assert_eq!(titles("z")[0], "Zen");
    assert_eq!(titles("st")[..2], ["Stocks", "Stickies"], "use tells two apart that start alike");
    let exact = titles("stickies");
    assert_eq!(exact[0], "Stickies", "use never outweighs the whole name: {exact:?}");
}

#[test]
fn open_app_edges_ahead() {
    let named = |answer: Answer| answer.rows.iter().map(|row| row.name.to_string()).collect::<Vec<_>>();
    let unused = named(asked("s"));
    let place = |names: &[String], title: &str| names.iter().position(|name| name == title).expect("listed");
    assert!(place(&unused, "Siri") < place(&unused, "Safari"), "with nothing open: {unused:?}");
    let browsing = named(asked_with("s", &["/Applications/Safari.app"]));
    assert!(place(&browsing, "Safari") < place(&browsing, "Siri"), "the open one is the likelier: {browsing:?}");
    assert_eq!(browsing[..2], ["Spotify", "Stocks"], "what is used every day still leads");
    assert_eq!(named(asked_with("sto", &["/Applications/Safari.app"]))[0], "Stocks", "it lifts only what was typed");
}

#[test]
fn paths_and_dates_skip_apps() {
    let below = titles("john/app");
    assert!(asked("john/app").rows.iter().all(|row| !row.app), "a folder was asked for, not every app: {below:?}");
    let listed = asked("apps");
    assert!(listed.rows.len() > 5 && listed.rows.iter().take(5).all(|row| row.app), "the word still lists them");
    let mut live = this_mac();
    put(&mut live, "/Applications/Fresh.app/", NOW - 60, 0);
    put(&mut live, "/Users/john/Documents/minutes.txt", NOW - 60, 0);
    let lately: Vec<String> = asked_on(&live, "today", &[]).rows.iter().map(|row| row.name.to_string()).collect();
    assert_eq!(lately, ["minutes.txt", "Fresh"], "an app changed today is listed, behind the file that was");
}

#[test]
fn apps_before_files_helpers_not() {
    let weather = asked("weather");
    let kinds: Vec<bool> = weather.rows.iter().map(|row| row.app).collect();
    assert_eq!(kinds, [true, false]);
    let creative = asked("creative");
    let order: Vec<(&str, bool)> = creative.rows.iter().take(2).map(|row| (&*row.name, row.app)).collect();
    assert_eq!(order, [("creative-brief.txt", false), ("Creative Cloud Content Manager", true)]);
}

#[test]
fn stray_app_leads_once_used() {
    let unused = asked("we");
    let order: Vec<(&str, bool)> = unused.rows.iter().take(2).map(|row| (&*row.name, row.app)).collect();
    assert_eq!(order, [("weather-report.pdf", false), ("Weather", true)], "never used, it ranks like a file");
    let used = asked("not");
    let order: Vec<(&str, bool)> = used.rows.iter().take(2).map(|row| (&*row.name, row.app)).collect();
    assert_eq!(order, [("Notepad", true), ("notes.md", false)], "used a few times, it leads like any app");
    assert_eq!(first("weather").0, "Weather", "and its whole name still finds it first");
}

#[test]
fn twin_apps_told_apart() {
    let siri = asked("siri");
    let apps: Vec<(&str, &str)> =
        siri.rows.iter().filter(|row| row.app).map(|row| (&*row.name, row.path.to_str().unwrap())).collect();
    assert_eq!(apps, [("Siri", "/System/Applications/Siri.app"), ("Siri AI", "/System/Applications/Siri AI.app")]);
    assert_eq!(siri.rows.last().map(|row| &*row.name), Some("siri-shortcuts.md"), "then the file");
}

#[test]
fn finder_and_nested_apps_count() {
    assert_eq!(first("finder"), ("Finder".into(), FINDER.into(), true, vec![(0, 6)]));
    assert_eq!(first("instr"), ("Instruments".into(), INSTRUMENTS.into(), true, vec![(0, 5)]));
    let listed = asked("apps");
    assert!(listed.rows.iter().any(|row| &*row.path == Path::new(INSTRUMENTS)), "it is listed among the apps");
}

#[test]
fn panes_found_by_shown_name() {
    assert_eq!(first("wifi"), ("Wi-Fi".into(), format!("{PANES}/Wi-Fi.appex"), true, vec![(0, 2), (3, 5)]));
    assert_eq!(first("wi-fi").0, "Wi-Fi");
    assert_eq!(first("blue").0, "Bluetooth");
    assert_eq!(first("display"), ("Displays".into(), format!("{PANES}/DisplaysExt.appex"), true, vec![(0, 7)]));
    assert!(titles("displaysext").is_empty(), "the name of a pane's file is nobody's business");
    let (about, intents) = (titles("about"), titles("settingsintents"));
    assert!(
        about.is_empty() && intents.is_empty(),
        "an extension that is no pane is not listed: {about:?} {intents:?}"
    );
    assert!(titles("neverread").is_empty(), "nor is one the book has not read");
    assert!(titles("extensionkit").is_empty() && titles("extensions").is_empty(), "nor the folders on the way");
    let listed = asked("apps");
    assert!(listed.rows.iter().any(|row| &*row.name == "Wi-Fi"), "a pane is listed among the apps");
    assert!(!listed.rows.iter().any(|row| row.name.contains("Intents") || row.name.contains("NeverRead")));
}

#[test]
fn seen_app_found_anywhere() {
    assert_eq!(first("windows 11"), ("Windows 11".into(), VM.into(), true, vec![(0, 7), (8, 10)]), "inside a package");
    assert_eq!(first("vm").1, VM, "and by the name of its file, where that starts");
    assert_eq!(titles("windows 11"), ["Windows 11"], "the package that holds it is not listed beside it");
    assert_eq!(first("win").1, VM, "an agent in a person's own place leads like any app once it was used");
    let ghost = asked("ghost");
    assert_eq!(ghost.rows.iter().map(|row| (&*row.name, row.app)).collect::<Vec<_>>(), [("Ghost Updater", true)]);
    assert!(titles("bettercmdtab").is_empty(), "a folder that only ends like an app is none");
    assert!(titles("unread").is_empty(), "and an app in an unusual place waits until it has been read");
    let listed = asked("apps");
    let place = |title: &str| listed.rows.iter().position(|row| &*row.name == title);
    assert!(place("Windows 11").is_some(), "the word lists it with the others");
    assert!(place("Unread Tool").is_none() && place("https+++bettercmdtab").is_none());
}

#[test]
fn slips_find_shown_name() {
    assert_eq!(first("waether").0, "Weather");
    assert_eq!(first("kynte").0, "Keynote");
    assert_eq!(first("spotfy").0, "Spotify");
    assert!(titles("xqzv").is_empty());
}

fn bundles_below(dir: &Path, depth: usize, found: &mut Vec<PathBuf>) {
    let Ok(listed) = std::fs::read_dir(dir) else { return };
    for entry in listed.flatten() {
        let path = entry.path();
        let bundle = path.extension().is_some_and(|ending| ending.eq_ignore_ascii_case("app"));
        if bundle {
            found.push(path);
        } else if depth > 0 && entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            bundles_below(&path, depth - 1, found);
        }
    }
}

#[test]
#[ignore = "reads the real apps of this machine and prints what a few questions find"]
fn this_machine_answers() {
    let home = std::env::var("HOME").expect("a home folder");
    let mut found = Vec::new();
    for place in ["/Applications", "/System/Applications", "/System/Library/CoreServices/Applications"] {
        bundles_below(Path::new(place), 2, &mut found);
    }
    bundles_below(&Path::new(&home).join("Applications"), 2, &mut found);
    bundles_below(&Path::new(&home).join("Applications (Parallels)"), 2, &mut found);
    found.push("/System/Library/CoreServices/Finder.app".into());
    let mut live = Live::empty();
    for bundle in &found {
        let linked = std::fs::symlink_metadata(bundle).is_ok_and(|seen| seen.file_type().is_symlink());
        let path = bundle.to_string_lossy();
        let planted = if linked { path.into_owned() } else { format!("{path}/") };
        put(&mut live, &planted, LONG_AGO, if linked { flag::LINK } else { 0 });
    }
    let listed: Vec<crate::find::apps::Listed> = found
        .iter()
        .map(|path| crate::find::apps::Listed { key: key_of(path), path: path.clone(), modified: LONG_AGO })
        .collect();
    let used = crate::find::apps::used();
    let book = Arc::new(Book::default().revised(&listed, used.as_ref(), crate::find::apps::read));
    println!("{} apps, {} with a card, use known for {}", found.len(), book.len(), used.map_or(0, |used| used.len()));
    let now = crate::find::context::now();
    for text in ["s", "c", "z", "sp", "we", "key", "find", "term", "photo", "siri", "sptoify", "vsc", "logic", "set"] {
        let ask = Ask { generation: 1, text: Arc::from(text), context: Context::at(now, 0, None) };
        let latest = AtomicU64::new(1);
        let mut scratch = Scratch::new(Some(PathBuf::from(&home)));
        scratch.book = book.clone();
        let usage = Usage::load(None, 0);
        let answer = run(&live, &usage, &ask, &mut scratch, &Stop { latest: &latest, mine: 1 }).expect("answered");
        let rows: Vec<&str> = answer.rows.iter().take(8).map(|row| &*row.name).collect();
        println!("{text:>8}: {rows:?}");
    }
}
