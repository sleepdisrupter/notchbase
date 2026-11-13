use std::cell::Cell;

use notchbase_sdk::ffi::{ABI_VERSION, entry_symbol, serve};
use notchbase_sdk::protocol::{
    Answer, Effects, Event, HandlerId, Node, Request, Start, Surface, TextNode, TimerMode, UiValue, Values,
};
use notchbase_sdk::{Driver, Runtime, Waker};

use super::*;
use prefs::{AUTO_BREAK, BREAK_MINUTES, FOCUS_MINUTES, SESSIONS};

const MINUTE: u64 = 60_000;
const START: u64 = 1_700_000_000_000;

thread_local! {
    static NOW: Cell<u64> = const { Cell::new(0) };
}

struct Fake;

impl Clock for Fake {
    fn now() -> u64 {
        NOW.with(Cell::get)
    }
}

type Focus = FocusTimer<Fake>;

fn at(now: u64) {
    NOW.with(|cell| cell.set(now));
}

fn values(pairs: &[(&str, Value)]) -> Values {
    pairs.iter().cloned().map(|(key, value)| (key.to_owned(), value)).collect()
}

fn started_with(pairs: &[(&str, Value)]) -> Runtime<Focus> {
    at(START);
    let mut runtime = Runtime::new(Waker::noop());
    runtime.start(Start { settings: values(pairs), ..Start::default() });
    runtime
}

fn started() -> Runtime<Focus> {
    started_with(&[])
}

fn tick(runtime: &mut Runtime<Focus>, now: u64) -> Vec<Request> {
    at(now);
    runtime.handle(vec![Event::Tick { every_ms: 1000 }]).requests
}

fn countdown(runtime: &Runtime<Focus>) -> Countdown {
    runtime.tool().expect("a started tool").countdown
}

fn sessions(runtime: &Runtime<Focus>) -> u64 {
    runtime.tool().expect("a started tool").sessions
}

fn page_of(runtime: &mut Runtime<Focus>) -> Node {
    runtime.views().page.expect("a page")
}

fn find<'a>(node: &'a Node, test: &impl Fn(&Node) -> bool) -> Option<&'a Node> {
    if test(node) {
        return Some(node);
    }
    node.children().into_iter().find_map(|child| find(child, test))
}

fn texts(node: &Node) -> Vec<String> {
    let mut found = Vec::new();
    collect_texts(node, &mut found);
    found
}

fn collect_texts(node: &Node, found: &mut Vec<String>) {
    if let Node::Text(TextNode { text, .. }) = node {
        found.push(text.clone());
    }
    node.children().into_iter().for_each(|child| collect_texts(child, found));
}

fn button_labeled(node: &Node, wanted: &str) -> Node {
    find(node, &|node| matches!(node, Node::Button { label, .. } if label == wanted))
        .cloned()
        .unwrap_or_else(|| panic!("no {wanted} button in {node:?}"))
}

fn segmented_of(node: &Node) -> Node {
    find(node, &|node| matches!(node, Node::Segmented { .. })).cloned().expect("a preset picker")
}

fn checkbox_of(node: &Node) -> Node {
    find(node, &|node| matches!(node, Node::Checkbox { .. })).cloned().expect("a checkbox")
}

fn ui(runtime: &mut Runtime<Focus>, surface: Surface, handler: Option<HandlerId>, value: UiValue) -> Effects {
    let handler = handler.expect("a live control");
    runtime.handle(vec![Event::Ui { surface, handler, value }])
}

fn press(runtime: &mut Runtime<Focus>, surface: Surface, handler: Option<HandlerId>) -> Effects {
    ui(runtime, surface, handler, UiValue::Press)
}

fn remaining_timer(at: u64, style: TextStyle) -> Node {
    Node::Timer { at, mode: TimerMode::Remaining, style, tone: Tone::Primary }
}

#[test]
fn manifest_has_page_and_shortcut() {
    let manifest = Focus::manifest();
    assert_eq!((manifest.id.as_str(), manifest.name.as_str()), (ID, "Focus"));
    assert_eq!(manifest.page.as_deref(), Some("Focus"));
    let [shortcut] = &manifest.shortcuts[..] else { panic!("{:?}", manifest.shortcuts) };
    assert_eq!(
        (shortcut.id.as_str(), shortcut.title.as_str(), shortcut.default.as_deref()),
        (TOGGLE, "Start or Pause Focus", Some("Ctrl+Alt+F"))
    );
    let keys: Vec<&str> = manifest.settings.iter().map(|setting| setting.key.as_str()).collect();
    assert_eq!(keys, [FOCUS_MINUTES, BREAK_MINUTES, AUTO_BREAK, SESSIONS]);
}

#[test]
fn entry_point_uses_crate_name() {
    assert_eq!(entry_symbol("focus_timer"), "notchbase_tool_focus_timer");
    let plugin = unsafe { &*__notchbase_tool_entry() };
    assert_eq!(plugin.abi, ABI_VERSION);
    let mut runtime = Runtime::<FocusTimer>::new(Waker::noop());
    let Answer::Manifest { manifest } = serve(&mut runtime, br#"{"call":"manifest"}"#) else { panic!("a manifest") };
    assert_eq!(manifest.id, ID);
}

#[test]
fn starts_idle_without_ticking() {
    at(START);
    let mut runtime = Runtime::<Focus>::new(Waker::noop());
    let effects = runtime.start(Start::default());
    assert!(effects.requests.is_empty());
    assert!(effects.wants.every_ms.is_empty());
    assert!(effects.wants.settings);
    assert_eq!(countdown(&runtime), Countdown::idle(25 * MINUTE));
    assert_eq!(sessions(&runtime), 0);
    assert!(runtime.views().live.is_none());
}

#[test]
fn toggle_starts_ticking_focus() {
    let mut runtime = started();
    let effects = runtime.send(Msg::Toggle);
    assert!(effects.requests.is_empty());
    assert_eq!(effects.wants.every_ms, vec![1000]);
    let running = countdown(&runtime);
    assert_eq!((running.phase(), running.end()), (Phase::Focus, Some(START + 25 * MINUTE)));
}

#[test]
fn pause_stops_the_ticks() {
    let mut runtime = started();
    runtime.send(Msg::Toggle);
    at(START + 10 * MINUTE);
    let effects = runtime.send(Msg::Toggle);
    assert!(effects.wants.every_ms.is_empty());
    assert_eq!(countdown(&runtime).run(), Run::Paused { left: 15 * MINUTE });
    at(START + 90 * MINUTE);
    let effects = runtime.send(Msg::Toggle);
    assert_eq!(effects.wants.every_ms, vec![1000]);
    assert_eq!(countdown(&runtime).end(), Some(START + 105 * MINUTE));
    assert_eq!(countdown(&runtime).phase(), Phase::Focus);
}

#[test]
fn shortcut_starts_and_pauses() {
    let mut runtime = started();
    let shortcut = |id: &str| vec![Event::Shortcut { id: id.into() }];
    runtime.handle(shortcut(TOGGLE));
    assert!(countdown(&runtime).running());
    at(START + 1_500);
    runtime.handle(shortcut(TOGGLE));
    assert_eq!(countdown(&runtime).run(), Run::Paused { left: 25 * MINUTE - 1_500 });
    runtime.handle(shortcut("something-else"));
    assert!(!countdown(&runtime).running());
}

#[test]
fn early_ticks_change_nothing() {
    let mut runtime = started();
    runtime.send(Msg::Toggle);
    assert!(tick(&mut runtime, START + 25 * MINUTE - 1).is_empty());
    assert_eq!(countdown(&runtime).phase(), Phase::Focus);
    assert_eq!(sessions(&runtime), 0);
}

#[test]
fn finished_focus_starts_break() {
    let mut runtime = started();
    runtime.send(Msg::Toggle);
    let done = START + 25 * MINUTE + 400;
    let requests = tick(&mut runtime, done);
    let [Request::SaveSetting { key, value }, Request::Notice { notice }] = &requests[..] else {
        panic!("{requests:?}")
    };
    assert_eq!((key.as_str(), value), (SESSIONS, &Value::from(1u64)));
    assert!(notice.pop);
    assert_eq!(notice.leading, Node::Badge { icon: Icon::Checkmark, size: 18.0, tone: Tone::Green });
    assert!(
        matches!(&notice.trailing, Node::Text(TextNode { text, style: TextStyle::Callout, .. }) if text == "Focus done")
    );
    let rest = countdown(&runtime);
    assert_eq!((rest.phase(), rest.end()), (Phase::Break, Some(done + 5 * MINUTE)));
    assert_eq!(runtime.handle(Vec::new()).wants.every_ms, vec![1000]);
    assert_eq!(sessions(&runtime), 1);
}

#[test]
fn finished_break_goes_idle() {
    let mut runtime = started();
    runtime.send(Msg::Toggle);
    tick(&mut runtime, START + 25 * MINUTE);
    let requests = tick(&mut runtime, START + 30 * MINUTE);
    let [Request::Notice { notice }] = &requests[..] else { panic!("{requests:?}") };
    assert!(notice.pop);
    assert_eq!(notice.leading, Node::Badge { icon: Icon::Bell, size: 18.0, tone: Tone::Orange });
    assert_eq!(texts(&notice.trailing), ["Break over"]);
    assert_eq!(countdown(&runtime), Countdown::idle(25 * MINUTE));
    assert!(runtime.handle(Vec::new()).wants.every_ms.is_empty());
    assert_eq!(sessions(&runtime), 1);
}

#[test]
fn no_auto_break_waits() {
    let mut runtime = started_with(&[(AUTO_BREAK, false.into())]);
    runtime.send(Msg::Toggle);
    let requests = tick(&mut runtime, START + 25 * MINUTE);
    assert!(matches!(&requests[..], [Request::SaveSetting { .. }, Request::Notice { .. }]));
    assert_eq!(countdown(&runtime), Countdown::idle(25 * MINUTE));
    assert!(runtime.views().live.is_none());
    runtime.send(Msg::Toggle);
    assert_eq!(countdown(&runtime).phase(), Phase::Focus);
}

#[test]
fn count_carries_on_from_saved() {
    let mut runtime = started_with(&[(SESSIONS, 4.into())]);
    assert!(texts(&page_of(&mut runtime)).contains(&"4 focus sessions done".to_owned()));
    runtime.send(Msg::Toggle);
    let requests = tick(&mut runtime, START + 25 * MINUTE);
    assert_eq!(requests[0], Request::SaveSetting { key: SESSIONS.into(), value: Value::from(5u64) });
    assert!(texts(&page_of(&mut runtime)).contains(&"5 focus sessions done".to_owned()));
}

#[test]
fn late_tick_times_break_from_now() {
    let mut runtime = started();
    runtime.send(Msg::Toggle);
    let late = START + 85 * MINUTE;
    assert_eq!(tick(&mut runtime, late).len(), 2);
    assert_eq!(countdown(&runtime).end(), Some(late + 5 * MINUTE));
    assert!(tick(&mut runtime, late + 1_000).is_empty());
    assert_eq!(sessions(&runtime), 1);
}

#[test]
fn paused_never_finishes() {
    let mut runtime = started();
    runtime.send(Msg::Toggle);
    at(START + MINUTE);
    runtime.send(Msg::Toggle);
    assert!(tick(&mut runtime, START + 600 * MINUTE).is_empty());
    assert!(runtime.send(Msg::Tick).requests.is_empty());
    assert_eq!(countdown(&runtime).run(), Run::Paused { left: 24 * MINUTE });
}

#[test]
fn settings_change_next_lengths() {
    let mut runtime = started();
    let settings = values(&[(FOCUS_MINUTES, 50.into()), (BREAK_MINUTES, 10.into())]);
    runtime.handle(vec![Event::Settings { values: settings }]);
    assert_eq!(countdown(&runtime), Countdown::idle(50 * MINUTE));
    let footer = runtime.views().tile.expect("a tile").footer;
    assert!(matches!(footer, Some(Node::Text(TextNode { text, .. })) if text == "50 min"));
    runtime.send(Msg::Toggle);
    assert_eq!(countdown(&runtime).end(), Some(START + 50 * MINUTE));
    tick(&mut runtime, START + 50 * MINUTE);
    assert_eq!(countdown(&runtime).end(), Some(START + 60 * MINUTE));
}

#[test]
fn running_session_keeps_length() {
    let mut runtime = started();
    runtime.send(Msg::Toggle);
    runtime.handle(vec![Event::Settings { values: values(&[(FOCUS_MINUTES, 15.into())]) }]);
    assert_eq!(countdown(&runtime).end(), Some(START + 25 * MINUTE));
    runtime.send(Msg::Reset);
    assert_eq!(countdown(&runtime), Countdown::idle(15 * MINUTE));
}

#[test]
fn host_settings_own_the_count() {
    let mut runtime = started_with(&[(SESSIONS, 2.into())]);
    runtime.handle(vec![Event::Settings { values: values(&[(SESSIONS, 9.into())]) }]);
    assert_eq!(sessions(&runtime), 9);
}

#[test]
fn presets_save_focus_length() {
    let mut runtime = started();
    let Node::Segmented { options, selected, on_select } = segmented_of(&page_of(&mut runtime)) else { unreachable!() };
    assert_eq!(options, ["15 min", "25 min", "50 min"]);
    assert_eq!(selected, 1);
    let effects = ui(&mut runtime, Surface::Page, on_select, UiValue::Index(2));
    assert_eq!(effects.requests, vec![Request::SaveSetting { key: FOCUS_MINUTES.into(), value: Value::from(50u64) }]);
    assert_eq!(countdown(&runtime), Countdown::idle(50 * MINUTE));
    let page = page_of(&mut runtime);
    assert!(texts(&page).contains(&"50:00".to_owned()));
    assert!(matches!(segmented_of(&page), Node::Segmented { selected: 2, .. }));
}

#[test]
fn custom_length_gets_a_segment() {
    let mut runtime = started_with(&[(FOCUS_MINUTES, 30.into())]);
    let Node::Segmented { options, selected, .. } = segmented_of(&page_of(&mut runtime)) else { unreachable!() };
    assert_eq!(options, ["15 min", "25 min", "50 min", "30 min"]);
    assert_eq!(selected, 3);
    assert!(runtime.send(Msg::Preset(3)).requests.is_empty());
    assert_eq!(runtime.tool().expect("a started tool").prefs.focus_minutes, 30);
}

#[test]
fn preset_mid_session_waits() {
    let mut runtime = started();
    runtime.send(Msg::Toggle);
    runtime.send(Msg::Preset(0));
    assert_eq!(countdown(&runtime).end(), Some(START + 25 * MINUTE));
    tick(&mut runtime, START + 25 * MINUTE);
    tick(&mut runtime, START + 30 * MINUTE);
    assert_eq!(countdown(&runtime), Countdown::idle(15 * MINUTE));
}

#[test]
fn checkbox_saves_auto_break() {
    let mut runtime = started();
    let Node::Checkbox { label, checked, on_toggle } = checkbox_of(&page_of(&mut runtime)) else { unreachable!() };
    assert_eq!((label.as_str(), checked), ("Start breaks automatically", true));
    let effects = ui(&mut runtime, Surface::Page, on_toggle, UiValue::Bool(false));
    assert_eq!(effects.requests, vec![Request::SaveSetting { key: AUTO_BREAK.into(), value: false.into() }]);
    assert!(matches!(checkbox_of(&page_of(&mut runtime)), Node::Checkbox { checked: false, .. }));
    runtime.send(Msg::Toggle);
    tick(&mut runtime, START + 25 * MINUTE);
    assert_eq!(countdown(&runtime).phase(), Phase::Idle);
}

#[test]
fn reset_goes_idle_from_break() {
    let mut runtime = started();
    runtime.send(Msg::Toggle);
    tick(&mut runtime, START + 25 * MINUTE);
    let Node::Button { on_press, .. } = button_labeled(&page_of(&mut runtime), "Reset") else { unreachable!() };
    let effects = press(&mut runtime, Surface::Page, on_press);
    assert!(effects.requests.is_empty());
    assert!(effects.wants.every_ms.is_empty());
    assert_eq!(countdown(&runtime), Countdown::idle(25 * MINUTE));
    assert_eq!(sessions(&runtime), 1);
}

#[test]
fn page_shows_countdown_and_controls() {
    let mut runtime = started();
    let idle = page_of(&mut runtime);
    let Node::Column { children, .. } = &idle else { panic!("{idle:?}") };
    assert_eq!(children.len(), 7);
    assert_eq!(texts(&children[0]), ["Ready"]);
    assert!(matches!(&children[1], Node::Text(TextNode { text, style: TextStyle::Display, .. }) if text == "25:00"));
    assert_eq!(children[2], Node::Progress { value: 0.0, tone: Tone::Secondary });
    assert!(matches!(
        button_labeled(&idle, "Start"),
        Node::Button { icon: Some(Icon::Play), style: ButtonStyle::Filled, on_press: Some(_), .. }
    ));
    assert!(matches!(button_labeled(&idle, "Reset"), Node::Button { on_press: None, .. }));
    assert!(idle.any(&|node| matches!(node, Node::Shortcut { id } if id == TOGGLE)));
    assert!(texts(&idle).contains(&"No focus sessions yet".to_owned()));
    let Node::Button { on_press, .. } = button_labeled(&idle, "Start") else { unreachable!() };
    press(&mut runtime, Surface::Page, on_press);
    at(START + 5 * MINUTE);
    let running = page_of(&mut runtime);
    let Node::Column { children, .. } = &running else { panic!("{running:?}") };
    assert_eq!(texts(&children[0]), ["Focus"]);
    assert_eq!(children[1], remaining_timer(START + 25 * MINUTE, TextStyle::Display));
    let Node::Progress { value, tone } = children[2] else { panic!("{:?}", children[2]) };
    assert!((value - 0.2).abs() < 1e-6);
    assert_eq!(tone, Tone::Orange);
    assert!(matches!(button_labeled(&running, "Reset"), Node::Button { on_press: Some(_), .. }));
    let Node::Button { on_press, icon, .. } = button_labeled(&running, "Pause") else { unreachable!() };
    assert_eq!(icon, Some(Icon::Pause));
    press(&mut runtime, Surface::Page, on_press);
    let paused = page_of(&mut runtime);
    let Node::Column { children, .. } = &paused else { panic!("{paused:?}") };
    assert_eq!(texts(&children[0]), ["Focus · Paused"]);
    assert_eq!(texts(&children[1]), ["20:00"]);
    assert!(matches!(button_labeled(&paused, "Resume"), Node::Button { style: ButtonStyle::Filled, .. }));
}

#[test]
fn tile_counts_down_while_running() {
    let mut runtime = started();
    let tile = runtime.views().tile.expect("a tile");
    assert_eq!((tile.title.as_str(), tile.wide, tile.shortcut.as_deref()), ("Focus", false, Some(TOGGLE)));
    assert!(matches!(tile.icon, Node::Icon { icon: Icon::Clock, .. }));
    assert!(matches!(&tile.footer, Some(Node::Text(TextNode { text, .. })) if text == "25 min"));
    assert_eq!((tile.well, tile.tint), (None, None));
    press(&mut runtime, Surface::Tile, tile.on_press);
    let tile = runtime.views().tile.expect("a tile");
    assert_eq!(tile.footer, Some(remaining_timer(START + 25 * MINUTE, TextStyle::Value)));
    assert_eq!((tile.well, tile.tint), (Some(Tone::Orange), Some(Tone::Orange)));
    assert!(matches!(tile.icon, Node::Icon { icon: Icon::Clock, tone: Tone::White, .. }));
    at(START + 90_500);
    press(&mut runtime, Surface::Tile, tile.on_press);
    let tile = runtime.views().tile.expect("a tile");
    assert!(matches!(&tile.footer, Some(Node::Text(TextNode { text, .. })) if text == "Paused · 23:30"));
    assert_eq!(tile.tint, None);
}

#[test]
fn live_shows_ring_and_time_left() {
    let mut runtime = started();
    assert!(runtime.views().live.is_none());
    runtime.send(Msg::Toggle);
    at(START + 5 * MINUTE);
    let live = runtime.views().live.expect("a live activity");
    let Node::Ring { value, size, tone, .. } = live.leading else { panic!("{:?}", live.leading) };
    assert!((value - 0.2).abs() < 1e-6);
    assert_eq!((size, tone), (13.0, Tone::Orange));
    assert_eq!(live.trailing, remaining_timer(START + 25 * MINUTE, TextStyle::Value));
    tick(&mut runtime, START + 25 * MINUTE);
    let live = runtime.views().live.expect("a break activity");
    assert_eq!(
        live.leading,
        Node::Ring { value: 0.0, size: 13.0, tone: Tone::Green, span: None, stroke: None, center: None }
    );
    assert_eq!(live.trailing, remaining_timer(START + 30 * MINUTE, TextStyle::Value));

    runtime.send(Msg::Toggle);
    assert!(runtime.views().live.is_none());
}
