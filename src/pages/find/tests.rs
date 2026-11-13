use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::{Event, Id, Key, Modifiers, Pos2};

use super::row::id_of;
use super::{COMPACT, FindPage};
use crate::bridge::{Action, FindRow, FindState, FindView, How, Kind, Pick};
use crate::pages::harness::{Harness, PAGE, key, press, release, typed};

const SETTLE: usize = 40;

mod rows;
mod spaces;

struct Run {
    harness: Harness,
    page: FindPage,
    view: FindView,
    typing: bool,
}

impl Run {
    fn new() -> Self {
        let view = FindView { state: FindState::Ready, entries: 500, ..FindView::default() };
        Self { harness: Harness::new(), page: FindPage::default(), view, typing: false }
    }

    fn frame(&mut self, events: Vec<Event>) -> Vec<Action> {
        let Self { harness, page, view, typing } = self;
        harness.frame(events, |ctx| page.show(ctx, view, *typing))
    }

    fn settle(&mut self) -> Vec<Action> {
        (0..SETTLE).flat_map(|_| self.frame(Vec::new())).collect()
    }

    fn enter(&mut self) -> Vec<Action> {
        self.page.arm();
        let mut actions = self.frame(Vec::new());
        self.page.browse();
        actions.extend(self.settle());
        actions
    }

    fn answer(&mut self, query: &str, names: &[&str]) {
        let rows: Vec<FindRow> = names.iter().map(|name| row(name)).collect();
        self.view =
            FindView { answered: self.view.answered + 1, query: query.into(), rows: rows.into(), ..self.view.clone() };
    }

    fn click(&mut self, id: Id) -> Vec<Action> {
        let at = self.harness.rect_of(id).center();
        let mut actions = self.frame(press(at));
        actions.extend(self.frame(release(at)));
        actions
    }
}

fn row(name: &str) -> FindRow {
    FindRow {
        key: name.bytes().fold(7, |key, byte| key * 31 + u64::from(byte)),
        path: Arc::from(Path::new("/Users/john").join(name).as_path()),
        name: name.into(),
        place: "~".into(),
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

fn opened(name: &str, typed: &str, rank: u8, how: How) -> Action {
    Action::FindPick(Pick {
        path: Path::new("/Users/john").join(name),
        typed: typed.into(),
        rank,
        how,
        predicted: false,
    })
}

fn with(key: Key, modifiers: Modifiers) -> Event {
    Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers }
}

#[test]
fn entering_takes_keyboard() {
    let mut run = Run::new();
    assert_eq!(run.enter(), [Action::FindShown(true), Action::Typing(true), Action::FindQuery(String::new())]);
    let mut left = Vec::new();
    run.page.present(false, &mut left);
    run.page.present(false, &mut left);
    assert_eq!(left, [Action::Typing(false), Action::FindShown(false)]);
    let mut quiet = Vec::new();
    run.enter();
    run.frame(vec![typed("ca")]);
    run.page.arm();
    run.page.drop_quietly();
    assert!(run.page.focus.is_none() && run.page.draft.is_empty() && !run.page.armed());
    run.page.present(false, &mut quiet);
    assert!(quiet.is_empty(), "{quiet:?}");
}

#[test]
fn typing_asks_once_per_edit() {
    let mut run = Run::new();
    run.enter();
    assert_eq!(run.frame(vec![typed("c")]), [Action::FindQuery("c".into())]);
    assert_eq!(run.frame(vec![key(Key::ArrowLeft), typed("a")]), [Action::FindQuery("ac".into())]);
    assert_eq!(run.frame(vec![key(Key::ArrowLeft)]), [], "moving the caret asks nothing");
}

#[test]
fn opens_as_bar_and_grows() {
    let bar = super::field::FIELD_H;
    let rows = |count: usize| bar + super::GAP + super::ROW_H * count as f32;
    let mut run = Run::new();
    assert_eq!(run.page.height_for(&run.view), bar, "a ready index opens as the bar alone");
    run.enter();
    run.page.browsing = false;
    run.answer("", &["a", "b", "c", "d", "e", "f"]);
    run.settle();
    assert_eq!(run.page.height_for(&run.view), bar, "suggestions wait until they are asked for");
    run.frame(vec![key(Key::ArrowDown)]);
    run.settle();
    assert_eq!(run.page.height_for(&run.view), rows(4), "the arrow asks, and one screen of them shows");
    run.frame(vec![typed("c")]);
    assert_eq!(run.page.height_for(&run.view), rows(4), "what stands is kept until the answer is in");
    run.answer("c", &["car", "cat"]);
    run.settle();
    assert_eq!(run.page.height_for(&run.view), rows(2), "two results, two rows");
    run.frame(vec![typed("z")]);
    run.answer("cz", &[]);
    run.settle();
    assert_eq!(run.page.height_for(&run.view), rows(1), "room to say that nothing was found");
    run.frame(vec![key(Key::Escape)]);
    run.settle();
    assert_eq!(run.page.height_for(&run.view), bar, "cleared, it is the bar again");
    assert_eq!(run.page.height_for(&FindView { entries: 0, ..run.view.clone() }), COMPACT);
    assert_eq!(run.page.height_for(&FindView { state: FindState::Starting, ..run.view.clone() }), COMPACT);
    assert_eq!(run.page.height_for(&FindView::default()), COMPACT);
}

#[test]
fn old_rows_never_painted() {
    let mut run = Run::new();
    run.answer("", &["old.txt"]);
    run.enter();
    assert!(!run.harness.drawn(id_of(row("old.txt").key)));
    run.answer("", &["new.txt"]);
    run.settle();
    assert!(run.harness.drawn(id_of(row("new.txt").key)));
}

#[test]
fn early_enter_waits_for_answer() {
    let mut run = Run::new();
    run.enter();
    run.answer("", &["a.txt"]);
    run.frame(vec![typed("car")]);
    assert_eq!(run.frame(vec![key(Key::Enter)]), [], "the rows on screen answer something else");
    run.answer("car", &["Cargo.toml", "Car.png"]);
    assert_eq!(run.frame(Vec::new()), [opened("Cargo.toml", "car", 0, How::Open)]);
    assert!(run.settle().is_empty(), "it fires once");
}

#[test]
fn held_enter_dropped_by_edit() {
    let mut run = Run::new();
    run.enter();
    run.frame(vec![typed("car"), key(Key::Enter)]);
    run.frame(vec![typed("g")]);
    run.answer("carg", &["Cargo.toml"]);
    assert!(run.settle().is_empty());
}

#[test]
fn refresh_keeps_rows_above() {
    let mut run = Run::new();
    run.enter();
    run.frame(vec![typed("ca")]);
    run.answer("ca", &["one", "two", "three"]);
    run.settle();
    run.frame(vec![key(Key::ArrowDown)]);
    run.answer("ca", &["new", "three", "one"]);
    run.settle();
    assert_eq!(run.frame(vec![key(Key::Enter)]), [opened("two", "ca", 1, How::Open)]);
    let keys: Vec<u64> = run.page.shown.rows.iter().map(|row| row.key).collect();
    assert_eq!(keys, ["one", "two", "new", "three"].map(|name| row(name).key));
}

#[test]
fn down_enter_opens_second_row() {
    let mut run = Run::new();
    run.enter();
    run.answer("", &["a", "b", "c"]);
    run.settle();
    assert_eq!(run.frame(vec![key(Key::ArrowDown), key(Key::Enter)]), [opened("b", "", 1, How::Open)]);
    assert_eq!(run.frame(vec![with(Key::Enter, Modifiers::COMMAND)]), [opened("b", "", 1, How::Reveal)]);
    run.frame(vec![key(Key::ArrowUp), key(Key::ArrowUp)]);
    assert_eq!(run.frame(vec![key(Key::Enter)]), [opened("a", "", 0, How::Open)]);
}

#[test]
fn escape_clears_then_closes() {
    let mut run = Run::new();
    run.enter();
    run.frame(vec![typed("car")]);
    assert_eq!(run.frame(vec![key(Key::Escape)]), [Action::FindQuery(String::new())]);
    assert!(run.page.focus.is_some(), "the keyboard is kept");
    assert_eq!(run.frame(vec![typed("x")]), [Action::FindQuery("x".into())]);
    run.frame(vec![key(Key::Escape)]);
    assert_eq!(run.frame(vec![key(Key::Escape)]), [Action::FindClose]);
}

#[test]
fn stale_keyboard_drops_caret() {
    let mut run = Run::new();
    run.enter();
    run.typing = true;
    run.frame(Vec::new());
    run.typing = false;
    let actions: Vec<Action> = (0..2).flat_map(|_| run.frame(Vec::new())).collect();
    assert!(actions.is_empty() && run.page.focus.is_none());
    assert!(run.frame(vec![typed("x")]).is_empty(), "keys go nowhere");
}

#[test]
fn no_matches_replaces_rows() {
    let mut run = Run::new();
    run.enter();
    run.frame(vec![typed("zz")]);
    run.answer("zz", &[]);
    run.settle();
    assert!(run.page.shown.rows.is_empty() && run.page.shown.answers("zz"));
    assert_eq!(run.frame(vec![key(Key::Enter)]), []);
}

#[test]
fn pinned_preview_asks_nothing() {
    let mut run = Run::new();
    run.page.pin(super::samples::results(), Some(1));
    assert!(run.settle().is_empty());
    assert_eq!(run.page.select.index, 1);
    assert!(run.page.focus.as_ref().is_some_and(|focus| focus.text() == "notch"));
    assert!(!run.page.armed());
}

#[test]
fn opening_grace_ends_by_clock() {
    let mut run = Run::new();
    run.page.arm();
    run.frame(Vec::new());
    assert!(run.page.settling(), "just opened");
    run.page.entered = Some(Instant::now() - Duration::from_secs(1));
    assert!(run.page.age < super::SETTLING, "barely a frame was painted");
    assert!(!run.page.settling(), "a panel that sat still for a second is no longer opening");
}

#[test]
fn leaving_gives_keyboard_back() {
    let mut run = Run::new();
    run.enter();
    run.page.focus = None;
    let mut left = Vec::new();
    run.page.rest(&mut left);
    assert_eq!(left, [Action::Typing(false), Action::FindShown(false)]);
    let mut again = Vec::new();
    run.page.rest(&mut again);
    assert!(again.is_empty(), "a page that already left says nothing");
    run.enter();
    let mut held = Vec::new();
    run.page.rest(&mut held);
    assert_eq!(held, [Action::Typing(false), Action::FindShown(false)], "with its caret it says so once");
}

mod pinned {
    use egui::{PointerButton, pos2};

    use super::*;
    use crate::bridge::Pinned;
    use crate::pages::find::pins;
    use crate::ui::menu;

    fn with_pins(names: &[&str]) -> Run {
        let mut run = Run::new();
        run.view.pins = names.iter().map(|name| row(name)).collect();
        run.page.arm();
        run.frame(Vec::new());
        run.settle();
        run
    }

    fn secondary(at: Pos2) -> [Vec<Event>; 2] {
        let button = |pressed| Event::PointerButton {
            pos: at,
            button: PointerButton::Secondary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        [vec![Event::PointerMoved(at), button(true)], vec![button(false)]]
    }

    fn right_click(run: &mut Run, id: Id) -> Vec<Action> {
        let at = run.harness.rect_of(id).center();
        secondary(at).into_iter().flat_map(|events| run.frame(events)).collect()
    }

    #[test]
    fn pins_show_until_typing() {
        let bar = super::super::field::FIELD_H;
        let strip = bar + super::super::GAP + pins::PINS_H;
        let mut run = with_pins(&["Zen", "notes.txt"]);
        assert_eq!(run.page.height_for(&run.view), strip);
        assert!(run.harness.drawn(pins::id_of(row("Zen").key)) && run.harness.drawn(pins::id_of(row("notes.txt").key)));
        run.frame(vec![typed("z")]);
        run.settle();
        assert!(!run.harness.drawn(pins::id_of(row("Zen").key)), "typing is searching");
        let mut bare = Run::new();
        bare.enter();
        bare.page.browsing = false;
        assert_eq!(bare.page.height_for(&bare.view), bar, "with nothing pinned it is the bar alone");
    }

    #[test]
    fn click_opens_pin() {
        let mut run = with_pins(&["Zen", "notes.txt"]);
        let actions = run.click(pins::id_of(row("notes.txt").key));
        assert_eq!(actions, [opened("notes.txt", "", 1, How::Open)]);
    }

    #[test]
    fn right_click_offers_pin_or_unpin() {
        let mut run = with_pins(&["Zen"]);
        run.frame(vec![typed("c")]);
        run.answer("c", &["car", "cat"]);
        run.settle();
        assert!(right_click(&mut run, id_of(row("cat").key)).is_empty(), "the menu only opens");
        assert!(run.harness.drawn(menu::item_id(0)));
        let pinned = run.click(menu::item_id(0));
        assert_eq!(pinned, [Action::FindPin(Pinned::of(&row("cat")))]);
        run.settle();
        assert!(!run.harness.drawn(menu::item_id(0)), "choosing closes it");
        let mut run = with_pins(&["Zen", "cat"]);
        right_click(&mut run, pins::id_of(row("Zen").key));
        assert_eq!(run.click(menu::item_id(0)), [Action::FindUnpin(row("Zen").path.to_path_buf())]);
    }

    #[test]
    fn menu_goes_on_press_or_key() {
        let mut run = with_pins(&["Zen"]);
        right_click(&mut run, pins::id_of(row("Zen").key));
        assert!(run.harness.drawn(menu::item_id(0)));
        assert!(run.page.room > run.page.height_for(&run.view), "the panel grows to hold the menu under the pointer");
        let elsewhere = pos2(2.0, 2.0);
        let mut actions = run.frame(press(elsewhere));
        actions.extend(run.frame(release(elsewhere)));
        run.settle();
        assert!(actions.is_empty() && !run.harness.drawn(menu::item_id(0)));
        assert_eq!(run.page.room, 0.0, "and gives the room back");
        right_click(&mut run, pins::id_of(row("Zen").key));
        assert_eq!(run.frame(vec![key(Key::Escape)]), [], "Escape closes the menu, not Find");
        run.settle();
        assert!(!run.harness.drawn(menu::item_id(0)));
    }

    #[test]
    fn menu_fades_in_place() {
        let mut run = with_pins(&["Zen"]);
        right_click(&mut run, pins::id_of(row("Zen").key));
        run.settle();
        let stood = run.page.menu.stands().expect("the menu is open");
        let with_strip = run.page.height_for(&run.view);
        assert!(stood.bottom() > PAGE.top() + with_strip, "it hangs below the strip it was opened on");
        assert_eq!(run.click(menu::item_id(0)), [Action::FindUnpin(row("Zen").path.to_path_buf())]);
        run.view.pins = Arc::from(Vec::new());
        run.frame(Vec::new());
        assert!(run.page.height_for(&run.view) < with_strip && run.page.room == 0.0, "the panel closes up at once");
        assert_eq!(run.page.menu.stands(), Some(stood), "but the menu is not pulled up with it");
        run.settle();
        assert_eq!(run.page.menu.stands(), None);
    }

    #[test]
    fn pin_drag_reorders_on_release() {
        let names = ["a", "b", "c", "d"];
        let mut run = with_pins(&names);
        let centre = |run: &Run, name: &str| run.harness.rect_of(pins::id_of(row(name).key)).center();
        let (from, to) = (centre(&run, "a"), centre(&run, "c"));
        let mut actions = run.frame(press(from));
        for step in 1..=8 {
            let at = from + (to - from) * (step as f32 / 8.0);
            actions.extend(run.frame(vec![Event::PointerMoved(at)]));
        }
        assert!(actions.is_empty(), "nothing is saved while it is in the hand");
        let dropped = run.frame(release(to));
        let order = ["b", "c", "a", "d"].map(|name| row(name).path.to_path_buf()).to_vec();
        assert_eq!(dropped, [Action::FindPinsOrder(order)]);
        let mut run = with_pins(&names);
        let home = centre(&run, "b");
        let mut nudged = run.frame(press(home));
        nudged.extend(run.frame(vec![Event::PointerMoved(home + egui::vec2(9.0, 0.0))]));
        nudged.extend(run.frame(release(home + egui::vec2(9.0, 0.0))));
        assert!(!nudged.iter().any(|action| matches!(action, Action::FindPinsOrder(_))), "back where it was");
    }
}
