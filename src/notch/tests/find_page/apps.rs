use egui::{Event, Key, Modifiers};

use crate::bridge::{FindRow, FindState, FindView, Kind, Why};
use crate::notch::Notch;
use crate::notch::preview::Preview;
use crate::platform::Style;

use super::super::fixtures::{Rig, key, shortcuts};
use super::{SETTLING_FRAMES, found, frames, open, row, words};

const SHORTCUT: &str = "/ProgramData/Microsoft/Windows/Start Menu/Programs/TreeSize Free/TreeSize Free.lnk";
const MENU: &str = "/ProgramData/Microsoft/Windows/Start Menu/Programs/TreeSize Free";
const IN_2019: u32 = 1_550_000_000;
const IN_2021: u32 = 1_620_000_000;

fn app() -> FindRow {
    FindRow {
        key: 1,
        name: "TreeSize Free".into(),
        place: MENU.into(),
        kind: Kind::App,
        app: true,
        modified: IN_2019,
        ..row(SHORTCUT)
    }
}

fn file(key: u64, path: &str, place: &str) -> FindRow {
    FindRow { key, place: place.into(), modified: IN_2021, ..row(path) }
}

fn listed(query: &str, rows: Vec<FindRow>) -> FindView {
    FindView {
        state: FindState::Ready,
        entries: 1_000,
        answered: 1,
        query: query.into(),
        rows: rows.into(),
        ..FindView::default()
    }
}

#[test]
fn app_listed_by_name() {
    let mut rig = Rig::new();
    let mut n = open(&rig);
    found(&rig, &mut n);
    rig.frame(&mut n, &shortcuts(None), vec![Event::Text("treesize".into())]);
    let installer = file(2, "/Users/john/Downloads/TreeSizeFreeSetup.exe", "~/Downloads");
    let shot = file(3, "/Users/john/Desktop/TreeSize_Free.png", "~/Desktop");
    rig.find = listed("treesize", vec![installer, app(), shot]);
    frames(&rig, &mut n, SETTLING_FRAMES);
    let down = |rig: &Rig, n: &mut Notch| {
        rig.frame(n, &shortcuts(None), vec![key(Key::ArrowDown, Modifiers::NONE)]);
        frames(rig, n, SETTLING_FRAMES);
        words(rig, n)
    };
    let resting = words(&rig, &mut n);
    let shows = |painted: &[String], wanted: &str| painted.iter().any(|word| word == wanted);
    assert!(shows(&resting, "TreeSize Free") && shows(&resting, "TreeSize_Free.png"), "{resting:?}");
    assert!(shows(&resting, "~/Downloads \u{b7} 2021"), "the selected row says where and when: {resting:?}");
    assert!(!shows(&resting, "App") && !shows(&resting, "~/Desktop \u{b7} 2021"), "the others only their names");
    let painted = down(&rig, &mut n);
    assert!(shows(&painted, "TreeSize Free") && shows(&painted, "App"), "{painted:?}");
    assert!(!painted.iter().any(|word| word.contains(".lnk") || word.contains("Start Menu")), "{painted:?}");
    assert!(!painted.iter().any(|word| word.contains("2019")), "when a shortcut was made is not shown: {painted:?}");
    let painted = down(&rig, &mut n);
    assert!(shows(&painted, "~/Desktop \u{b7} 2021") && !shows(&painted, "App"), "{painted:?}");
}

#[test]
fn suggested_app_says_why() {
    let mut rig = Rig::new();
    let mut n = open(&rig);
    found(&rig, &mut n);
    let notes = FindRow { why: Some(Why::AroundNow), ..file(2, "/Users/john/Documents/notes.md", "~/Documents") };
    rig.find = listed("", vec![notes, FindRow { why: Some(Why::With("Xcode".into())), ..app() }]);
    frames(&rig, &mut n, SETTLING_FRAMES);
    let painted = words(&rig, &mut n);
    let shows = |painted: &[String], wanted: &str| painted.iter().any(|word| word == wanted);
    assert!(shows(&painted, "TreeSize Free") && shows(&painted, "Often around now \u{b7} Documents"), "{painted:?}");
    assert!(!shows(&painted, "With Xcode \u{b7} App"), "a row that is not selected keeps its reason to itself");
    rig.frame(&mut n, &shortcuts(None), vec![key(Key::ArrowDown, Modifiers::NONE)]);
    frames(&rig, &mut n, SETTLING_FRAMES);
    let painted = words(&rig, &mut n);
    assert!(
        shows(&painted, "With Xcode \u{b7} App") && !painted.iter().any(|word| word.contains("2019")),
        "{painted:?}"
    );
}

#[test]
fn results_preview_leads_with_app() {
    let rig = Rig::new();
    let mut n = Notch::with_preview(Style::Island, None, Some(Preview::FindResults));
    frames(&rig, &mut n, SETTLING_FRAMES);
    let painted = words(&rig, &mut n);
    let titles: Vec<&str> = painted.iter().map(String::as_str).filter(|word| word.starts_with("Notch")).collect();
    assert_eq!(titles, ["NotchNook", "Notch.md"], "the app first, by its name alone: {painted:?}");
    assert!(painted.iter().any(|word| word == "App") && !painted.iter().any(|word| word == "~/Applications"));
}
