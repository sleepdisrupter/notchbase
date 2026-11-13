use egui::{Pos2, Rect, pos2, vec2};
use notchbase_sdk::prelude::*;
use notchbase_sdk::protocol::{Surface, UiValue};

use crate::bridge::{Action, Page, ToolSlot};
use crate::notch::Notch;
use crate::notch::preview::Preview;
use crate::platform::Style;

use super::fixtures::{Rig, shortcuts, tool_view};

const SLOT: ToolSlot = ToolSlot(9);
const WIDTH: f32 = 170.0;
const HEIGHT: f32 = 90.0;

pub(in crate::notch) struct Sketch;

#[derive(Clone, Debug, PartialEq)]
pub(in crate::notch) enum Msg {
    Pointed(Pointer),
    Scrolled(Pointer),
}

impl Tool for Sketch {
    type Msg = Msg;

    fn manifest() -> Manifest {
        Manifest::new("sketch", "Sketch").page("Sketch")
    }

    fn init(_: &Context) -> (Self, Command<Msg>) {
        (Self, Command::none())
    }

    fn update(&mut self, _: Msg) -> Command<Msg> {
        Command::none()
    }

    fn page(&self) -> Option<Element<Msg>> {
        let sky = draw::linear(Color::rgb(28, 30, 64), Color::rgb(236, 84, 90), 90.0);
        let wave = draw::curve((0.0, 70.0), (50.0, 40.0), (110.0, 95.0), (WIDTH, 60.0));
        let sea = wave.iter().copied().chain([(WIDTH, HEIGHT), (0.0, HEIGHT)]);
        let badge = draw::group([
            draw::glass(0.0, 0.0, 64.0, 22.0).radius(11.0),
            draw::icon(Icon::Clock, 12.0, 11.0).size(11.0).color(Tone::Yellow),
            draw::text("12:06", 22.0, 11.0).size(11.0).semibold().anchor(Anchor::Left),
        ]);
        let drawing = canvas(WIDTH, HEIGHT)
            .push(draw::rect(0.0, 0.0, WIDTH, HEIGHT).radius(14.0).fill(sky))
            .push(draw::shadow(118.0, 12.0, 28.0, 28.0).radius(14.0).blur(14.0).color(Tone::Yellow))
            .push(draw::circle(132.0, 26.0, 13.0).fill(draw::radial(Tone::White, Tone::Yellow)))
            .push(draw::polygon(sea).fill(Color::rgba(10, 12, 30, 200)).clip(0.0, 0.0, WIDTH, HEIGHT))
            .push(draw::line(wave).stroke(1.5, Tone::White).opacity(0.6))
            .push(draw::line(draw::arc(132.0, 26.0, 19.0, 200.0, 340.0)).stroke(1.0, Tone::Secondary))
            .push(draw::ellipse(40.0, 30.0, 16.0, 6.0).stroke(1.0, Tone::Tertiary))
            .push(badge.offset(10.0, 8.0))
            .push(draw::text("Free drawing", 85.0, 80.0).size(9.0).mono().color(Tone::Secondary).anchor(Anchor::Bottom))
            .push(draw::text("wraps onto more lines", 150.0, 50.0).size(8.0).anchor(Anchor::TopRight).wrap(50.0))
            .push(draw::image(ImageSource::File("missing.png".into()), 8.0, 60.0, 20.0, 14.0).radius(4.0).scale(0.5))
            .stretch()
            .on_pointer(Msg::Pointed)
            .on_scroll(Msg::Scrolled);
        Some(drawing.into())
    }
}

fn opened() -> (Rig, Notch, Rect) {
    let mut rig = Rig::new();
    rig.tools.push(tool_view::<Sketch>(SLOT.0));
    let mut n = Notch::with_preview(Style::Island, None, Some(Preview::Expanded));
    n.pager.show(Page::Tool(SLOT));
    for _ in 0..3 {
        rig.frame(&mut n, &shortcuts(None), Vec::new());
    }
    let drawn = n.content_rect(Page::Tool(SLOT));
    (rig, n, drawn)
}

fn button(at: Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

fn pointers(actions: &[Action]) -> Vec<Pointer> {
    let pointer = |action: &Action| match action {
        Action::Tool(SLOT, Surface::Page, _, UiValue::Pointer(pointer)) => Some(*pointer),
        _ => None,
    };
    actions.iter().filter_map(pointer).collect()
}

fn kinds(actions: &[Action]) -> Vec<PointerKind> {
    pointers(actions).iter().map(|pointer| pointer.kind).collect()
}

#[test]
fn stretched_canvas_keeps_shape() {
    let (_, _, drawn) = opened();
    assert_eq!(drawn.width(), 340.0);
    assert_eq!(drawn.height(), 180.0, "twice as wide, so twice as tall");
}

#[test]
fn pointer_in_canvas_units() {
    let (rig, mut n, drawn) = opened();
    let keys = shortcuts(None);
    let at = drawn.min + vec2(100.0, 60.0);
    let hovered = rig.frame(&mut n, &keys, vec![egui::Event::PointerMoved(at)]);
    assert_eq!(pointers(&hovered), [Pointer::at(PointerKind::Hover, 50.0, 30.0)]);
    assert!(pointers(&rig.frame(&mut n, &keys, Vec::new())).is_empty(), "a still pointer says nothing more");
    let pressed = rig.frame(&mut n, &keys, vec![button(at, true)]);
    assert_eq!(pointers(&pressed), [Pointer::at(PointerKind::Press, 50.0, 30.0)]);
    let released = rig.frame(&mut n, &keys, vec![button(at, false)]);
    assert_eq!(kinds(&released), [PointerKind::Release, PointerKind::Click]);
    let left = rig.frame(&mut n, &keys, vec![egui::Event::PointerMoved(pos2(2.0, 590.0))]);
    assert_eq!(kinds(&left), [PointerKind::Leave]);
}

#[test]
fn drag_reports_moves_no_click() {
    let (rig, mut n, drawn) = opened();
    let keys = shortcuts(None);
    let from = drawn.center();
    rig.frame(&mut n, &keys, vec![egui::Event::PointerMoved(from)]);
    let mut actions = rig.frame(&mut n, &keys, vec![button(from, true)]);
    for step in 1..=6 {
        let at = from + vec2(step as f32 * 6.0, step as f32 * 2.0);
        actions.extend(rig.frame(&mut n, &keys, vec![egui::Event::PointerMoved(at)]));
    }
    actions.extend(rig.frame(&mut n, &keys, vec![button(from + vec2(36.0, 12.0), false)]));
    let seen = kinds(&actions);
    assert_eq!((seen.first(), seen.last()), (Some(&PointerKind::Press), Some(&PointerKind::Release)));
    assert!(!seen.contains(&PointerKind::Click) && !seen.contains(&PointerKind::Hover), "{seen:?}");
    let drags: Vec<Pointer> = pointers(&actions).into_iter().filter(|p| p.kind == PointerKind::Drag).collect();
    let (dx, dy): (f32, f32) = drags.iter().fold((0.0, 0.0), |(dx, dy), drag| (dx + drag.dx, dy + drag.dy));
    assert!((12.0..=18.0).contains(&dx) && (4.0..=6.0).contains(&dy), "{dx} {dy} of {drags:?}");
    let last = drags.last().expect("a drag");
    assert!(last.near(85.0 + 18.0, 45.0 + 6.0, 0.5), "{last:?}");
}

#[test]
fn scroll_reported_both_ways() {
    let (rig, mut n, drawn) = opened();
    let keys = shortcuts(None);
    let wheel = egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: vec2(-20.0, 8.0),
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::NONE,
    };
    rig.frame(&mut n, &keys, vec![egui::Event::PointerMoved(drawn.min + vec2(20.0, 20.0))]);
    let scrolled = pointers(&rig.frame(&mut n, &keys, vec![wheel]));
    assert_eq!(scrolled, [Pointer::moved(PointerKind::Scroll, [10.0, 10.0], [-10.0, 4.0])]);
}
