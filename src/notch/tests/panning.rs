use egui::{Pos2, Rect, pos2, vec2};
use notchbase_sdk::prelude::*;
use notchbase_sdk::protocol::{Surface, UiValue};

use crate::bridge::{Action, Page, ToolSlot};
use crate::notch::Notch;
use crate::notch::preview::Preview;
use crate::platform::Style;

use super::fixtures::{Rig, shortcuts, tool_view};

const SLOT: ToolSlot = ToolSlot(9);
const CELL: f32 = 40.0;
const CELLS: usize = 20;
const SLID: f32 = -15.0;

struct Strip;

#[derive(Clone, Debug, PartialEq)]
enum Msg {
    Moved(f32),
    Tapped(f32),
}

impl Tool for Strip {
    type Msg = Msg;

    fn manifest() -> Manifest {
        Manifest::new("strip", "Strip").page("Strip")
    }

    fn init(_: &Context) -> (Self, Command<Msg>) {
        (Self, Command::none())
    }

    fn update(&mut self, _: Msg) -> Command<Msg> {
        Command::none()
    }

    fn page(&self) -> Option<Element<Msg>> {
        let cells = (0..CELLS).map(|cell| text(cell.to_string()).width(CELL));
        Some(pan(row(cells).spacing(0.0)).offset(SLID).on_pan(Msg::Moved).on_tap(Msg::Tapped).into())
    }
}

fn opened() -> (Rig, Notch, Rect) {
    let mut rig = Rig::new();
    rig.tools.push(tool_view::<Strip>(SLOT.0));
    let mut n = Notch::with_preview(Style::Island, None, Some(Preview::Expanded));
    n.pager.show(Page::Tool(SLOT));
    for _ in 0..3 {
        rig.frame(&mut n, &shortcuts(None), Vec::new());
    }
    let strip = n.content_rect(Page::Tool(SLOT));
    (rig, n, strip)
}

fn button(at: Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

fn heard(actions: &[Action], msg: fn(f32) -> Msg) -> Vec<f32> {
    let which = match msg(0.0) {
        Msg::Moved(_) => 0,
        Msg::Tapped(_) => 1,
    };
    let numbers = actions.iter().filter_map(|action| match action {
        Action::Tool(SLOT, Surface::Page, handler, UiValue::Number(number)) if handler.0 == which => Some(*number),
        _ => None,
    });
    numbers.collect()
}

#[test]
fn strip_fills_page() {
    let (_, _, strip) = opened();
    assert_eq!(strip.width(), 340.0);
    assert!(strip.height() > 10.0 && strip.height() < 24.0, "{strip:?}");
}

#[test]
fn drag_reports_distance() {
    let (rig, mut n, strip) = opened();
    let keys = shortcuts(None);
    let from = strip.center();
    rig.frame(&mut n, &keys, vec![egui::Event::PointerMoved(from)]);
    let mut actions = rig.frame(&mut n, &keys, vec![button(from, true)]);
    for step in 1..=8 {
        let at = from - vec2(step as f32 * 5.0, 0.0);
        actions.extend(rig.frame(&mut n, &keys, vec![egui::Event::PointerMoved(at)]));
    }
    actions.extend(rig.frame(&mut n, &keys, vec![button(from - vec2(40.0, 0.0), false)]));
    let moved: f32 = heard(&actions, Msg::Moved).iter().sum();
    assert!((-40.0..=-25.0).contains(&moved), "{moved} of {actions:?}");
    assert!(heard(&actions, Msg::Tapped).is_empty(), "a drag is not a click");
}

#[test]
fn click_reports_position() {
    let (rig, mut n, strip) = opened();
    let keys = shortcuts(None);
    let at = pos2(strip.center().x + 50.0, strip.center().y);
    rig.frame(&mut n, &keys, vec![egui::Event::PointerMoved(at)]);
    let mut actions = rig.frame(&mut n, &keys, vec![button(at, true)]);
    actions.extend(rig.frame(&mut n, &keys, vec![button(at, false)]));
    let middle = CELLS as f32 * CELL * 0.5;
    assert_eq!(heard(&actions, Msg::Tapped), [middle + 50.0 - SLID]);
    assert!(heard(&actions, Msg::Moved).is_empty());
}

#[test]
fn sideways_swipe_slides_strip() {
    let (rig, mut n, strip) = opened();
    let keys = shortcuts(None);
    let wheel = |x: f32, y: f32| egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: vec2(x, y),
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::NONE,
    };
    rig.frame(&mut n, &keys, vec![egui::Event::PointerMoved(strip.center())]);
    let sideways = rig.frame(&mut n, &keys, vec![wheel(-24.0, 3.0)]);
    assert_eq!(heard(&sideways, Msg::Moved), [-24.0]);
    let upward = rig.frame(&mut n, &keys, vec![wheel(2.0, 30.0)]);
    assert!(heard(&upward, Msg::Moved).is_empty());
}
