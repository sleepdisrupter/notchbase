use egui::{Pos2, pos2};
use notchbase_sdk::protocol::{Surface, UiValue};

use crate::bridge::{Action, Page, ToolSlot};
use crate::notch::Notch;
use crate::notch::preview::Preview;
use crate::platform::Style;

use super::fixtures::{Rig, shortcuts};

const COLUMNS: f32 = 4.0;
const WELL_CENTER: f32 = 26.0;
const PASSING_FRAMES: usize = 12;
const RESTING_FRAMES: usize = 24;
const SETTLING_FRAMES: usize = 90;
const PEEKING: f32 = 5.125;
const LINGERING_FRAMES: usize = 60;

fn opened(rig: &Rig) -> Notch {
    let mut n = Notch::with_preview(Style::Island, None, Some(Preview::Expanded));
    for _ in 0..3 {
        rig.frame(&mut n, &shortcuts(None), Vec::new());
    }
    n
}

fn icon(n: &Notch, index: usize) -> Pos2 {
    let page = n.content_rect(Page::Tools);
    let width = page.width() / COLUMNS;
    pos2(page.left() + (index as f32 + 0.5) * width, page.top() + WELL_CENTER)
}

fn click(rig: &Rig, n: &mut Notch, at: Pos2) -> Vec<Action> {
    let button = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let keys = shortcuts(None);
    let mut actions = rig.frame(n, &keys, vec![egui::Event::PointerMoved(at)]);
    actions.extend(rig.frame(n, &keys, vec![button(true)]));
    actions.extend(rig.frame(n, &keys, vec![button(false)]));
    actions
}

#[test]
fn opens_on_icons_alone() {
    let rig = Rig::new();
    let n = opened(&rig);
    assert_eq!(n.pages.tools.focused(), None);
    assert_eq!(n.content_rect(Page::Tools).height(), 70.0, "one row of icons and no card");
}

#[test]
fn click_runs_tool_without_card() {
    let rig = Rig::new();
    let mut n = opened(&rig);
    let at = icon(&n, 1);
    let actions = click(&rig, &mut n, at);
    let pressed =
        actions.iter().any(|action| matches!(action, Action::Tool(ToolSlot(1), Surface::Tile, _, UiValue::Press)));
    assert!(pressed, "{actions:?}");
    assert_eq!(n.pages.tools.focused(), None);
}

#[test]
fn lingering_opens_card() {
    let rig = Rig::new();
    let mut n = opened(&rig);
    let keys = shortcuts(None);
    let at = icon(&n, 2);
    let before = n.content_rect(Page::Tools).height();
    rig.frame(&mut n, &keys, vec![egui::Event::PointerMoved(at)]);
    for _ in 0..PASSING_FRAMES {
        rig.frame(&mut n, &keys, Vec::new());
    }
    assert_eq!(n.pages.tools.focused(), None, "a pointer on its way to a click opens nothing");
    for _ in 0..RESTING_FRAMES {
        rig.frame(&mut n, &keys, Vec::new());
    }
    assert_eq!(n.pages.tools.focused(), Some(ToolSlot(2)));
    for _ in 0..SETTLING_FRAMES {
        rig.frame(&mut n, &keys, Vec::new());
    }
    assert_eq!(n.content_rect(Page::Tools).height(), before + 90.0, "the gap and the card");
    let next = icon(&n, 0);
    rig.frame(&mut n, &keys, vec![egui::Event::PointerMoved(next)]);
    for _ in 0..PASSING_FRAMES {
        rig.frame(&mut n, &keys, Vec::new());
    }
    assert_eq!(n.pages.tools.focused(), Some(ToolSlot(0)), "an open card follows the pointer sooner");
}

#[test]
fn rebinding_shows_the_tool() {
    let rig = Rig::new();
    let mut n = opened(&rig);
    for _ in 0..3 {
        rig.frame(&mut n, &shortcuts(Some(1)), Vec::new());
    }
    assert_eq!(n.pages.tools.focused(), Some(ToolSlot(1)));
}

fn drag(rig: &Rig, n: &mut Notch, from: Pos2, to: Pos2) -> Vec<Action> {
    let button = |at: Pos2, pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let keys = shortcuts(None);
    let mut actions = rig.frame(n, &keys, vec![egui::Event::PointerMoved(from)]);
    actions.extend(rig.frame(n, &keys, vec![button(from, true)]));
    for step in 1..=10 {
        let at = from + (to - from) * (step as f32 / 10.0);
        actions.extend(rig.frame(n, &keys, vec![egui::Event::PointerMoved(at)]));
    }
    actions.extend(rig.frame(n, &keys, vec![button(to, false)]));
    actions
}

fn moves(actions: &[Action]) -> Vec<(ToolSlot, Option<ToolSlot>)> {
    let moved = |action: &Action| match action {
        Action::MoveTool(slot, before) => Some((*slot, *before)),
        _ => None,
    };
    actions.iter().filter_map(moved).collect()
}

#[test]
fn drag_moves_tool_to_slot() {
    let rig = Rig::new();
    let mut n = opened(&rig);
    let (first, second, third) = (icon(&n, 0), icon(&n, 1), icon(&n, 2));
    let actions = drag(&rig, &mut n, first, second);
    assert_eq!(moves(&actions), [(ToolSlot(0), Some(ToolSlot(2)))]);
    let ran = actions.iter().any(|action| matches!(action, Action::Tool(..) | Action::OpenToolPage(_)));
    assert!(!ran, "a drag is not a click: {actions:?}");
    assert_eq!(n.pages.tools.focused(), None, "and it opens no card on the way");
    let mut n = opened(&rig);
    assert_eq!(moves(&drag(&rig, &mut n, first, third)), [(ToolSlot(0), None)]);
}

#[test]
fn putting_back_moves_nothing() {
    let rig = Rig::new();
    let mut n = opened(&rig);
    let (first, second) = (icon(&n, 0), icon(&n, 1));
    let keys = shortcuts(None);
    let out = drag(&rig, &mut n, second, first + egui::vec2(0.0, 30.0));
    assert_eq!(moves(&out), [(ToolSlot(1), Some(ToolSlot(0)))]);
    let mut n = opened(&rig);
    rig.frame(&mut n, &keys, vec![egui::Event::PointerMoved(second)]);
    let there_and_back = drag(&rig, &mut n, second, second + egui::vec2(9.0, 4.0));
    assert!(moves(&there_and_back).is_empty(), "{there_and_back:?}");
}

fn crowded() -> (Rig, Notch) {
    let mut rig = Rig::with_every_tile();
    for slot in 5..9 {
        rig.tools.push(super::fixtures::tool_view::<::timer::Timer>(slot));
    }
    let n = opened(&rig);
    (rig, n)
}

fn swipe(x: f32) -> egui::Event {
    egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(x, 0.0),
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::NONE,
    }
}

#[test]
fn crowded_row_scrolls_sideways() {
    let (rig, mut n) = crowded();
    let keys = shortcuts(None);
    assert_eq!(n.content_rect(Page::Tools).height(), 70.0);
    let row = n.content_rect(Page::Tools);
    rig.frame(&mut n, &keys, vec![egui::Event::PointerMoved(row.center())]);
    rig.frame(&mut n, &keys, vec![swipe(-80.0)]);
    assert_eq!(n.pages.tools.scroll(), 80.0);
    assert_eq!(n.pager.page, Page::Tools, "the swipe moved the row, not the page");
    rig.frame(&mut n, &keys, vec![swipe(-5000.0)]);
    let end = 9.0 * row.width() / PEEKING - row.width();
    assert!((n.pages.tools.scroll() - end).abs() < 0.01, "it stops at the last tool: {}", n.pages.tools.scroll());
    rig.frame(&mut n, &keys, vec![swipe(9000.0)]);
    assert_eq!(n.pages.tools.scroll(), 0.0);
}

#[test]
fn fitting_row_ignores_swipes() {
    let rig = Rig::new();
    let mut n = opened(&rig);
    let keys = shortcuts(None);
    let over = icon(&n, 1);
    rig.frame(&mut n, &keys, vec![egui::Event::PointerMoved(over)]);
    rig.frame(&mut n, &keys, vec![swipe(-80.0)]);
    assert_eq!(n.pages.tools.scroll(), 0.0);
}

#[test]
fn click_reaches_scrolled_icon() {
    let (rig, mut n) = crowded();
    let row = n.content_rect(Page::Tools);
    let first = pos2(row.left() + 30.0, row.top() + WELL_CENTER);
    n.pages.tools.scroll_to(2.0 * row.width() / PEEKING);
    let actions = click(&rig, &mut n, first);
    let pressed = |slot: u16| {
        actions
            .iter()
            .any(|action| matches!(action, Action::Tool(ToolSlot(at), Surface::Tile, _, UiValue::Press) if *at == slot))
    };
    assert!(pressed(2) && !pressed(0), "{actions:?}");
}

#[test]
fn edge_hold_scrolls_and_drops() {
    let (rig, mut n) = crowded();
    let keys = shortcuts(None);
    let row = n.content_rect(Page::Tools);
    let (from, edge) = (icon_at(row, 1), pos2(row.right() - 3.0, row.top() + WELL_CENTER));
    let button = |at: Pos2, pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    rig.frame(&mut n, &keys, vec![egui::Event::PointerMoved(from)]);
    rig.frame(&mut n, &keys, vec![button(from, true)]);
    for step in 1..=10 {
        rig.frame(&mut n, &keys, vec![egui::Event::PointerMoved(from + (edge - from) * (step as f32 / 10.0))]);
    }
    for _ in 0..30 {
        rig.frame(&mut n, &keys, Vec::new());
    }
    assert!(n.pages.tools.scroll() > 100.0, "{}", n.pages.tools.scroll());
    let dropped = rig.frame(&mut n, &keys, vec![button(edge, false)]);
    let [(moved, before)] = moves(&dropped)[..] else { panic!("{dropped:?}") };
    assert_eq!(moved, ToolSlot(1));
    assert!(before.is_none_or(|before| before.0 >= 6), "it landed well past where a still row ends: {before:?}");
}

fn icon_at(row: egui::Rect, index: usize) -> Pos2 {
    pos2(row.left() + (index as f32 + 0.5) * row.width() / PEEKING, row.top() + WELL_CENTER)
}

#[test]
fn empty_row_without_tools() {
    let mut rig = Rig::new();
    rig.tools.clear();
    let n = opened(&rig);
    assert_eq!(n.content_rect(Page::Tools).height(), 70.0, "one row, as with tools");
}

fn tint_first(rig: &mut Rig, tint: Option<notchbase_sdk::protocol::Tone>) {
    if let Some(tile) = rig.tools[0].tile.as_mut() {
        tile.tint = tint;
    }
}

fn opened_for_real(rig: &Rig) -> Notch {
    let mut n = super::fixtures::island();
    n.open_page(Page::Tools);
    for _ in 0..SETTLING_FRAMES {
        rig.frame(&mut n, &shortcuts(None), Vec::new());
    }
    n
}

#[test]
fn running_row_slides_in() {
    let mut rig = Rig::new();
    let mut n = opened_for_real(&rig);
    let keys = shortcuts(None);
    let before = n.content_rect(Page::Tools).height();
    for tint in [Some(notchbase_sdk::protocol::Tone::Orange), None] {
        tint_first(&mut rig, tint);
        let mut heights = vec![n.content_rect(Page::Tools).height()];
        for _ in 0..SETTLING_FRAMES {
            rig.frame(&mut n, &keys, Vec::new());
            heights.push(n.content_rect(Page::Tools).height());
        }
        let row = crate::pages::tools::running_slot();
        let expected = if tint.is_some() { before + row } else { before };
        assert!((heights[SETTLING_FRAMES] - expected).abs() < 0.01, "{heights:?}");
        for pair in heights.windows(2) {
            assert!((pair[1] - pair[0]).abs() < row * 0.35, "it moves over several frames: {heights:?}");
        }
    }
}

#[test]
fn running_row_already_there() {
    let mut rig = Rig::new();
    tint_first(&mut rig, Some(notchbase_sdk::protocol::Tone::Orange));
    let mut n = super::fixtures::island();
    n.open_page(Page::Tools);
    let full = 70.0 + crate::pages::tools::running_slot();
    for _ in 0..SETTLING_FRAMES {
        rig.frame(&mut n, &shortcuts(None), Vec::new());
        assert_eq!(n.content_rect(Page::Tools).height(), full, "no row slides in after the panel opens");
    }
}

#[test]
fn opening_card_stays_inside() {
    let rig = Rig::new();
    let mut n = opened_for_real(&rig);
    let keys = shortcuts(None);
    for _ in 0..SETTLING_FRAMES {
        rig.frame(&mut n, &keys, Vec::new());
    }
    let at = icon(&n, 2);
    rig.frame(&mut n, &keys, vec![egui::Event::PointerMoved(at)]);
    let mut worst = f32::MAX;
    for _ in 0..(LINGERING_FRAMES + SETTLING_FRAMES) {
        rig.frame(&mut n, &keys, Vec::new());
        let room = n.silhouette().rect().bottom() - n.content_rect(Page::Tools).bottom();
        worst = worst.min(room);
    }
    assert_eq!(n.pages.tools.focused(), Some(ToolSlot(2)));
    assert!(worst > 0.0, "the card's edge never pokes past the panel's: {worst}");
}
