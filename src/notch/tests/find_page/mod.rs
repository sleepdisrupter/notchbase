use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use egui::{Event, Key, Modifiers, Shape};

use crate::bridge::{Action, FindRow, FindState, FindView, How, Kind, Page, Pick};
use crate::notch::Notch;

use super::fixtures::{AWAY, Rig, click, island, key, ms, notch, shortcuts};

mod apps;
mod handover;
mod launching;
mod spaces;

const SETTLING_FRAMES: usize = 120;
const COMPACT: f32 = 150.0;
const BAR: f32 = 46.0;
const ROW: f32 = 52.0;
const GAP: f32 = 8.0;
const SWITCH_FROM_RIGHT: f32 = 34.0;
const TITLE_ABOVE: f32 = 8.0;

fn frames(rig: &Rig, n: &mut Notch, count: usize) -> Vec<Action> {
    let keys = shortcuts(None);
    (0..count).flat_map(|_| rig.frame(n, &keys, Vec::new())).collect()
}

fn open(rig: &Rig) -> Notch {
    let mut n = island();
    n.open_page(Page::Tools);
    frames(rig, &mut n, SETTLING_FRAMES);
    n
}

fn found(rig: &Rig, n: &mut Notch) -> Vec<Action> {
    let at = n.header().find.center();
    let mut actions = click(rig, n, at);
    actions.extend(frames(rig, n, SETTLING_FRAMES));
    n.pages.find.browse();
    actions
}

fn row(path: &str) -> FindRow {
    let path = Path::new(path);
    FindRow {
        key: 0,
        path: Arc::from(path),
        name: path.file_name().unwrap_or_default().to_string_lossy().into(),
        place: Arc::from("~"),
        marks: Vec::new(),
        kind: Kind::Other,
        app: false,
        folder: false,
        cloud: false,
        modified: 0,
        why: None,
        predicted: false,
        icon: None,
    }
}

fn answered(query: &str, paths: &[&str]) -> FindView {
    let rows: Vec<FindRow> = paths.iter().map(|path| row(path)).collect();
    FindView {
        state: FindState::Ready,
        entries: 1_000,
        answered: 1,
        query: Arc::from(query),
        rows: rows.into(),
        ..FindView::default()
    }
}

fn words(rig: &Rig, n: &mut Notch) -> Vec<String> {
    fn gather(shape: &Shape, found: &mut Vec<String>) {
        match shape {
            Shape::Vec(shapes) => shapes.iter().for_each(|shape| gather(shape, found)),
            Shape::Text(text) => found.push(text.galley.text().to_owned()),
            _ => {}
        }
    }
    let (_, mut output) = rig.run(n, &shortcuts(None), Vec::new(), 1.0 / 60.0);
    output.textures_delta.clear();
    let mut found = Vec::new();
    output.shapes.iter().for_each(|clipped| gather(&clipped.shape, &mut found));
    found
}

#[test]
fn button_opens_find_with_keyboard() {
    let rig = Rig::new();
    for mut n in [open(&rig), notch()] {
        n.open_page(Page::Tools);
        frames(&rig, &mut n, SETTLING_FRAMES);
        let actions = found(&rig, &mut n);
        assert_eq!(actions, [Action::FindShown(true), Action::Typing(true), Action::FindQuery(String::new())]);
        assert_eq!((n.pager.page, n.visible_page()), (Page::Find, Some(Page::Find)));
        assert!(!n.pages.find.armed(), "armed once per press");
    }
}

#[test]
fn system_key_opens_find() {
    let rig = Rig::new();
    let entered = [Action::FindShown(true), Action::Typing(true), Action::FindQuery(String::new())];
    for mut n in [island(), notch()] {
        assert!(n.is_collapsed() && !n.finding());
        n.summon_find();
        let keys = shortcuts(None);
        assert_eq!(rig.frame(&mut n, &keys, Vec::new()), entered, "on its first frame, before anything shows");
        assert!(n.panel_alpha < 0.01, "the panel is still to come into view");
        n.set_pinned(true);
        let typed = rig.frame(&mut n, &keys, vec![Event::Text("s".into())]);
        assert_eq!(typed, [Action::FindQuery("s".into())], "what is typed while it opens is not lost");
        assert!(frames(&rig, &mut n, SETTLING_FRAMES).is_empty());
        assert!(n.finding() && n.visible_page() == Some(Page::Find));
        n.summon_find();
        assert!(frames(&rig, &mut n, 2).is_empty() && n.finding(), "summoned twice, it is entered once");
    }
    let mut n = open(&rig);
    n.summon_find();
    assert_eq!(frames(&rig, &mut n, SETTLING_FRAMES), entered, "from the tools page");
    assert_eq!((n.pager.page, n.pager.from, n.finding()), (Page::Find, Page::Tools, true));
}

#[test]
fn quick_second_press_keeps_find() {
    let rig = Rig::new();
    let mut n = open(&rig);
    let at = n.header().find.center();
    let mut actions = click(&rig, &mut n, at);
    actions.extend(frames(&rig, &mut n, 6));
    assert!(n.finding() && n.find_settling(), "a tenth of a second in, Find is still opening");
    actions.extend(click(&rig, &mut n, at));
    actions.extend(frames(&rig, &mut n, SETTLING_FRAMES));
    assert_eq!((n.pager.page, n.finding()), (Page::Find, true), "the second click of a double click is not a wish");
    assert!(!actions.contains(&Action::FindShown(false)) && !actions.contains(&Action::Typing(false)));
    assert!(!n.find_settling(), "two seconds later it has settled");
    click(&rig, &mut n, at);
    assert_eq!((n.pager.page, n.pager.from), (Page::Tools, Page::Find), "and a press now goes back as it always did");
}

#[test]
fn summon_counts_as_opening_briefly() {
    let rig = Rig::new();
    let mut n = island();
    assert!(!n.find_settling());
    n.summon_find();
    assert!(n.find_settling(), "armed, before any frame");
    rig.frame(&mut n, &shortcuts(None), Vec::new());
    n.set_pinned(true);
    assert!(n.find_settling());
    frames(&rig, &mut n, 27);
    assert!(n.find_settling(), "half a second has not passed");
    frames(&rig, &mut n, 4);
    assert!(n.finding() && !n.find_settling());
}

#[test]
fn locked_notch_ignores_summon() {
    let rig = Rig::new();
    let mut n = island();
    n.set_locked(true);
    n.summon_find();
    assert!(!n.finding() && !n.pages.find.armed());
    assert!(!takes_the_keyboard(&frames(&rig, &mut n, SETTLING_FRAMES)));
    n.set_locked(false);
    n.set_pinned(true);
    let actions = frames(&rig, &mut n, SETTLING_FRAMES);
    assert!(!takes_the_keyboard(&actions), "unlocking opens a tab, not the Find nobody saw: {actions:?}");
}

#[test]
fn every_keystroke_asks_again() {
    let rig = Rig::new();
    let mut n = open(&rig);
    found(&rig, &mut n);
    let keys = shortcuts(None);
    assert_eq!(rig.frame(&mut n, &keys, vec![Event::Text("c".into())]), [Action::FindQuery("c".into())]);
    assert_eq!(rig.frame(&mut n, &keys, vec![Event::Text("ar".into())]), [Action::FindQuery("car".into())]);
    let erase = key(Key::Backspace, Modifiers::NONE);
    assert_eq!(rig.frame(&mut n, &keys, vec![erase]), [Action::FindQuery("ca".into())]);
    assert!(frames(&rig, &mut n, 3).is_empty(), "and nothing is asked while nothing is typed");
}

#[test]
fn names_listed_four_at_a_time() {
    let mut rig = Rig::new();
    let mut n = open(&rig);
    found(&rig, &mut n);
    assert!(!words(&rig, &mut n).iter().any(|word| word.ends_with(".txt")));
    let paths: Vec<String> = (1..=8).map(|file| format!("/Users/john/note-{file}.txt")).collect();
    rig.find = answered("", &paths.iter().map(String::as_str).collect::<Vec<_>>());
    frames(&rig, &mut n, SETTLING_FRAMES);
    let listed: Vec<String> = words(&rig, &mut n).into_iter().filter(|word| word.ends_with(".txt")).collect();
    assert_eq!(listed, ["note-1.txt", "note-2.txt", "note-3.txt", "note-4.txt"]);
}

#[test]
fn compact_until_index_has_rows() {
    let mut rig = Rig::new();
    let mut n = open(&rig);
    found(&rig, &mut n);
    assert_eq!(n.content_rect(Page::Find).height(), COMPACT);
    rig.find = FindView { state: FindState::Ready, ..FindView::default() };
    frames(&rig, &mut n, 2);
    assert_eq!(n.content_rect(Page::Find).height(), COMPACT, "ready, but nothing indexed yet");
    rig.find = answered("", &["/Users/john/a.txt"]);
    frames(&rig, &mut n, 2);
    assert_eq!(n.content_rect(Page::Find).height(), BAR + GAP + ROW, "one row for one result");
    let five: Vec<String> = (1..=5).map(|file| format!("/Users/john/{file}.txt")).collect();
    let mut rig = Rig::new();
    let mut n = open(&rig);
    found(&rig, &mut n);
    rig.find = answered("", &five.iter().map(String::as_str).collect::<Vec<_>>());
    frames(&rig, &mut n, 2);
    assert_eq!(n.content_rect(Page::Find).height(), BAR + GAP + 4.0 * ROW, "and never more than one screen");
}

#[test]
fn enter_opens_first_row() {
    let mut rig = Rig::new();
    let mut n = open(&rig);
    found(&rig, &mut n);
    let keys = shortcuts(None);
    rig.frame(&mut n, &keys, vec![Event::Text("car".into())]);
    rig.find = answered("car", &["/Users/john/Cargo.toml", "/Users/john/Car.png"]);
    frames(&rig, &mut n, 2);
    let actions = rig.frame(&mut n, &keys, vec![key(Key::Enter, Modifiers::NONE)]);
    let pick =
        Pick { path: "/Users/john/Cargo.toml".into(), typed: "car".into(), rank: 0, how: How::Open, predicted: false };
    assert!(actions.contains(&Action::FindPick(pick)), "{actions:?}");
    assert!(!actions.iter().any(|action| matches!(action, Action::FindQuery(_))), "opening asks nothing new");
}

#[test]
fn escape_on_empty_closes_once() {
    let rig = Rig::new();
    let mut n = open(&rig);
    found(&rig, &mut n);
    let actions = rig.frame(&mut n, &shortcuts(None), vec![key(Key::Escape, Modifiers::NONE)]);
    assert_eq!(actions.iter().filter(|action| **action == Action::FindClose).count(), 1, "{actions:?}");
    assert!(frames(&rig, &mut n, SETTLING_FRAMES).is_empty(), "and it is said once");
}

#[test]
fn hiding_gives_keyboard_back_once() {
    let rig = Rig::new();
    for mut n in opened(&rig) {
        pointer_leaves(&mut n);
        let actions = frames(&rig, &mut n, SETTLING_FRAMES);
        assert!(n.is_collapsed());
        assert_eq!(actions, [Action::Typing(false), Action::FindShown(false)]);
    }
}

#[test]
fn click_beside_find_passes_through() {
    let rig = Rig::new();
    for n in opened(&rig) {
        let panel = n.bounds(n.expanded_geometry());
        assert!(n.finding() && n.wants_input(panel.center()));
        let off = [
            egui::pos2(panel.left() - 12.0, panel.center().y),
            egui::pos2(panel.right() + 12.0, panel.center().y),
            egui::pos2(panel.center().x, panel.bottom() + 12.0),
            AWAY,
        ];
        for at in off {
            assert!(n.canvas_rect().contains(at), "{at:?} is over the notch's window");
            assert!(!n.wants_input(at), "{at:?} beside {panel:?} goes to what is under it");
        }
    }
}

fn opened(rig: &Rig) -> [Notch; 2] {
    [island(), notch()].map(|mut n| {
        n.open_page(Page::Tools);
        frames(rig, &mut n, SETTLING_FRAMES);
        found(rig, &mut n);
        n
    })
}

fn pointer_leaves(n: &mut Notch) {
    let t0 = Instant::now();
    n.tick(t0, Some(AWAY));
    n.tick(t0 + ms(300), Some(AWAY));
}

fn takes_the_keyboard(actions: &[Action]) -> bool {
    actions.iter().any(|action| matches!(action, Action::FindShown(true) | Action::Typing(true)))
}

#[test]
fn reopening_mid_fade_lands_on_tab() {
    let rig = Rig::new();
    for mut n in opened(&rig) {
        pointer_leaves(&mut n);
        frames(&rig, &mut n, 2);
        assert!(n.panel_alpha > 0.1 && !n.is_collapsed(), "still fading out");
        n.set_pinned(true);
        let actions = frames(&rig, &mut n, SETTLING_FRAMES);
        assert_eq!(n.visible_page(), Some(Page::Tools));
        assert!(!takes_the_keyboard(&actions), "{actions:?}");
    }
}

#[test]
fn hidden_panel_never_reopens_find() {
    let rig = Rig::new();
    for mut n in opened(&rig) {
        pointer_leaves(&mut n);
        frames(&rig, &mut n, SETTLING_FRAMES);
        assert!(n.is_collapsed());
        assert_eq!(n.pager.page, Page::Tools, "it lets go of Find as it hides");
        n.set_pinned(true);
        let actions = frames(&rig, &mut n, SETTLING_FRAMES);
        assert_eq!(n.visible_page(), Some(Page::Tools));
        assert!(!takes_the_keyboard(&actions), "{actions:?}");
    }
}

#[test]
fn explicit_open_survives_first_frame() {
    let rig = Rig::new();
    let mut n = island();
    n.open_page(Page::Tools);
    n.toggle_find();
    rig.frame(&mut n, &shortcuts(None), Vec::new());
    assert_eq!(n.pager.page, Page::Find, "an armed page survives the frame that opens the panel");
    let mut n = open(&rig);
    let at = n.header().find.center();
    let mut actions = click(&rig, &mut n, at);
    for _ in 0..SETTLING_FRAMES {
        actions.extend(rig.frame(&mut n, &shortcuts(None), Vec::new()));
        assert_eq!(n.pager.page, Page::Find);
    }
    assert!(actions.contains(&Action::FindShown(true)));
}

#[test]
fn leaving_for_other_display_is_quiet() {
    let rig = Rig::new();
    for mut n in opened(&rig) {
        n.leave_find();
        let actions = frames(&rig, &mut n, SETTLING_FRAMES);
        assert!(actions.is_empty(), "{actions:?}");
        assert_eq!(n.visible_page(), Some(Page::Tools));
    }
}

#[test]
fn settings_card_offers_find_and_clear() {
    let mut rig = Rig::new();
    let mut n = open(&rig);
    rig.find = FindView { state: FindState::Ready, entries: 1_452_310, ..FindView::default() };
    n.open_page(Page::Settings);
    frames(&rig, &mut n, SETTLING_FRAMES);
    let tile = rig.ctx.read_response(egui::Id::new(("notchbase.settings.tile", "Find"))).expect("the Find tile");
    click(&rig, &mut n, tile.rect.center());
    frames(&rig, &mut n, SETTLING_FRAMES);
    let painted = words(&rig, &mut n);
    for row in ["Find files", "1,452,310 items", "What Find learned", "Clear"] {
        assert!(painted.iter().any(|word| word == row), "{row} in {painted:?}");
    }
    assert!(painted.iter().any(|word| word.starts_with("Stays on this ")));
    let clear = spot(&rig, &mut n, "Clear").expect("the Clear pill");
    assert!(click(&rig, &mut n, clear).contains(&Action::FindForget));
    let title = spot(&rig, &mut n, "Find files").expect("the Find row");
    let switch = egui::pos2(n.content_rect(Page::Settings).right() - SWITCH_FROM_RIGHT, title.y + TITLE_ABOVE);
    assert!(click(&rig, &mut n, switch).contains(&Action::SetFindEnabled(false)));
}

fn spot(rig: &Rig, n: &mut Notch, wanted: &str) -> Option<egui::Pos2> {
    fn gather(shape: &Shape, wanted: &str) -> Option<egui::Pos2> {
        match shape {
            Shape::Vec(shapes) => shapes.iter().find_map(|shape| gather(shape, wanted)),
            Shape::Text(text) if text.galley.text() == wanted => Some(text.visual_bounding_rect().center()),
            _ => None,
        }
    }
    let (_, mut output) = rig.run(n, &shortcuts(None), Vec::new(), 1.0 / 60.0);
    output.textures_delta.clear();
    output.shapes.iter().find_map(|clipped| gather(&clipped.shape, wanted))
}

#[test]
fn second_press_goes_back_to_tab() {
    let rig = Rig::new();
    let mut n = open(&rig);
    found(&rig, &mut n);
    let at = n.header().find.center();
    click(&rig, &mut n, at);
    assert_eq!((n.pager.page, n.pager.from), (Page::Tools, Page::Find));
}
