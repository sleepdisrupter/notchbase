use egui::{Event, Id, Key, Modifiers, Pos2};

use crate::bridge::{Action, Page};
use crate::notch::Notch;

use super::fixtures::{Rig, click, island, key, notch, shortcuts};

const SETTLING_FRAMES: usize = 120;
const SEARCH: &str = "notchbase.settings.search";

fn frames(rig: &Rig, n: &mut Notch, count: usize) -> Vec<Action> {
    let keys = shortcuts(None);
    (0..count).flat_map(|_| rig.frame(n, &keys, Vec::new())).collect()
}

fn opened(rig: &Rig, mut n: Notch) -> Notch {
    n.open_page(Page::Settings);
    frames(rig, &mut n, SETTLING_FRAMES);
    n
}

fn at(rig: &Rig, id: Id) -> Pos2 {
    rig.ctx.read_response(id).unwrap_or_else(|| panic!("{id:?} is drawn")).rect.center()
}

fn drawn(rig: &Rig, id: Id) -> bool {
    rig.ctx.read_response(id).is_some()
}

fn tile(title: &str) -> Id {
    Id::new(("notchbase.settings.tile", title))
}

fn press(rig: &Rig, n: &mut Notch, id: Id) -> Vec<Action> {
    let mut actions = click(rig, n, at(rig, id));
    actions.extend(frames(rig, n, SETTLING_FRAMES));
    actions
}

fn typed(rig: &Rig, n: &mut Notch, events: Vec<Event>) -> Vec<Action> {
    let mut actions = rig.frame(n, &shortcuts(None), events);
    actions.extend(frames(rig, n, SETTLING_FRAMES));
    actions
}

fn searching(rig: &Rig, text: &str) -> Notch {
    let mut n = opened(rig, island());
    let actions = press(rig, &mut n, Id::new(SEARCH));
    assert!(actions.contains(&Action::Typing(true)), "the field takes the keyboard");
    typed(rig, &mut n, vec![Event::Text(text.into())]);
    n
}

#[test]
fn tile_opens_its_rows() {
    let rig = Rig::new();
    for n in [island(), notch()] {
        let mut n = opened(&rig, n);
        assert!(drawn(&rig, tile("Popups")) && drawn(&rig, Id::new(SEARCH)));
        assert!(!drawn(&rig, Id::new("notchbase.settings.volume")));
        press(&rig, &mut n, tile("Popups"));
        let volume = Id::new("notchbase.settings.volume");
        assert!(drawn(&rig, volume) && !drawn(&rig, tile("Popups")), "the tile's view slid in");
        assert_eq!(press(&rig, &mut n, volume), [Action::SetVolumeIndicator(false)]);
        press(&rig, &mut n, Id::new(("notchbase.settings.back", "Settings")));
        assert!(drawn(&rig, tile("Popups")), "and back home");
    }
}

#[test]
fn quit_and_notifications_tiles() {
    let rig = Rig::new();
    let mut n = opened(&rig, island());
    assert!(press(&rig, &mut n, Id::new("notchbase.settings.quit")).contains(&Action::Quit));
    press(&rig, &mut n, tile("Notifications"));
    assert_eq!(n.pager.page, Page::Notifications);
}

#[test]
fn typing_finds_rows_that_work() {
    let rig = Rig::new();
    let mut n = searching(&rig, "displays");
    let displays = Id::new("notchbase.settings.displays");
    assert!(drawn(&rig, displays) && !drawn(&rig, tile("Popups")), "results take the tiles' place");
    let actions = press(&rig, &mut n, displays);
    assert!(actions.contains(&Action::SetAllDisplays(true)), "{actions:?}");
    assert!(drawn(&rig, displays), "the search stays after a click in the results");
}

#[test]
fn tool_settings_are_found() {
    let rig = Rig::new();
    searching(&rig, "screenshots");
    let copy = Id::new(("notchbase.settings.tool", 1u16, "copy"));
    assert!(drawn(&rig, copy), "the Screenshot tool's own setting shows");
}

#[test]
fn chip_narrows_to_group() {
    let rig = Rig::new();
    let mut n = searching(&rig, "screen");
    let displays = Id::new("notchbase.settings.displays");
    assert!(drawn(&rig, displays));
    press(&rig, &mut n, Id::new(("notchbase.settings.chip", "Tools")));
    assert!(!drawn(&rig, displays), "only Tools' results");
    press(&rig, &mut n, Id::new(("notchbase.settings.chip", "All")));
    assert!(drawn(&rig, displays));
}

#[test]
fn escape_empties_then_lets_go() {
    let rig = Rig::new();
    let mut n = searching(&rig, "displays");
    let escape = key(Key::Escape, Modifiers::NONE);
    assert!(!typed(&rig, &mut n, vec![escape.clone()]).contains(&Action::Typing(false)));
    assert!(drawn(&rig, tile("Popups")), "the first Escape empties the search");
    assert!(typed(&rig, &mut n, vec![escape]).contains(&Action::Typing(false)), "the second lets the keyboard go");
}

#[test]
fn nothing_found_says_so() {
    let rig = Rig::new();
    searching(&rig, "zzzz");
    assert!(!drawn(&rig, tile("Popups")) && !drawn(&rig, Id::new(("notchbase.settings.chip", "Notch"))));
    assert!(drawn(&rig, Id::new(("notchbase.settings.chip", "All"))));
}
