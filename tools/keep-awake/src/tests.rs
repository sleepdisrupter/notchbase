use std::cell::Cell;

use notchbase_sdk::protocol::{Node, Request, TimerMode};
use notchbase_sdk::testing::Harness;

use super::*;

const START: u64 = 1_700_000_000_000;

thread_local! {
    static NOW: Cell<u64> = const { Cell::new(START) };
}

struct Fake;

impl Clock for Fake {
    fn now() -> u64 {
        NOW.with(Cell::get)
    }
}

type Tested = KeepAwake<Fake>;

fn on() -> [Request; 2] {
    [Request::KeepAwake { awake: true }, saved("forever")]
}

fn off() -> [Request; 2] {
    [Request::KeepAwake { awake: false }, saved("")]
}

fn saved(value: &str) -> Request {
    Request::SaveSetting { key: SESSION.into(), value: value.into() }
}

fn at(now: u64) {
    NOW.with(|cell| cell.set(now));
}

fn started(length: &str) -> Harness<Tested> {
    at(START);
    Harness::start_with([(LENGTH, length)])
}

#[test]
fn tile_press_keeps_awake() {
    let mut harness = started("0");
    assert!(harness.live().is_none());
    assert!(harness.shows("Until I turn it off"));
    assert_eq!(harness.press_tile(), on());
    let tile = harness.tile().unwrap();
    assert_eq!((tile.well, tile.tint), (Some(Tone::Orange), Some(Tone::Orange)));
    assert!(harness.shows(AWAKE));
    assert_eq!(harness.press_tile(), off());
    assert!(harness.live().is_none() && harness.tile().unwrap().well.is_none());
}

#[test]
fn timed_session_ends_on_time() {
    let mut harness = started("30");
    assert!(harness.shows("30 minutes"));
    harness.press_tile();
    let until = START + 30 * MINUTE_MS;
    let live = harness.live().expect("a live activity");
    assert!(matches!(live.trailing, Node::Timer { at, mode: TimerMode::Remaining, .. } if at == until));
    at(until - 1);
    assert!(harness.tick(TICK).is_empty());
    at(until);
    assert!(matches!(
        harness.tick(TICK),
        [Request::KeepAwake { awake: false }, Request::SaveSetting { .. }, Request::Notice { .. }]
    ));
    assert_eq!(harness.noticed(), [ENDED]);
    assert!(harness.live().is_none());
}

#[test]
fn ticks_only_while_timed() {
    let mut harness = started("15");
    assert!(harness.wants().every_ms.is_empty());
    harness.press_tile();
    assert_eq!(harness.wants().every_ms, [1000]);
    harness.press_tile();
    harness.change_settings([(LENGTH, "0")]);
    harness.press_tile();
    assert!(harness.wants().every_ms.is_empty(), "an open-ended session needs no clock");
}

#[test]
fn shortcut_toggles_live_stops() {
    let mut harness = started("0");
    assert_eq!(harness.shortcut(TOGGLE), on());
    assert_eq!(harness.click_icon(Icon::Xmark, 0), off());
    assert!(harness.live().is_none());
}

#[test]
fn cup_can_stay_hidden() {
    let mut harness = Harness::<Tested>::start_with([(IN_NOTCH, false)]);
    assert_eq!(harness.press_tile(), on());
    assert!(harness.live().is_none());
    assert!(harness.tile().unwrap().well.is_some(), "the tile still shows that it is on");
}

#[test]
fn new_length_waits_for_next_press() {
    let mut harness = started("15");
    harness.press_tile();
    harness.change_settings([(LENGTH, "60")]);
    let Some(Node::Timer { at, .. }) = harness.live().map(|live| live.trailing) else { panic!("a countdown") };
    assert_eq!(at, START + 15 * MINUTE_MS);
    harness.press_tile();
    harness.press_tile();
    let Some(Node::Timer { at, .. }) = harness.live().map(|live| live.trailing) else { panic!("a countdown") };
    assert_eq!(at, START + 60 * MINUTE_MS);
}

#[test]
fn timed_session_survives_restart() {
    at(START);
    let until = (START + 30 * MINUTE_MS).to_string();
    let mut harness = Harness::<Tested>::start_with([(SESSION, until.as_str())]);
    assert_eq!(harness.requests(), [Request::KeepAwake { awake: true }]);
    let Some(Node::Timer { at, .. }) = harness.live().map(|live| live.trailing) else { panic!("a countdown") };
    assert_eq!(at, START + 30 * MINUTE_MS);
    assert_eq!(harness.wants().every_ms, [1000]);
    assert!(harness.tile().unwrap().well.is_some());
}

#[test]
fn open_session_survives_restart() {
    let mut harness = Harness::<Tested>::start_with([(SESSION, FOREVER)]);
    assert_eq!(harness.requests(), [Request::KeepAwake { awake: true }]);
    assert!(harness.shows(AWAKE));
}

#[test]
fn ended_while_closed_is_forgotten() {
    at(START);
    let until = (START - 1).to_string();
    let mut harness = Harness::<Tested>::start_with([(SESSION, until.as_str())]);
    assert_eq!(harness.requests(), [saved("")]);
    assert!(harness.live().is_none() && harness.tile().unwrap().well.is_none());
}

#[test]
fn forced_stays_awake() {
    let mut harness = Harness::<Tested>::start_with([(FORCED, true)]);
    assert!(harness.shows(ALWAYS));
    assert!(harness.tile().unwrap().well.is_some());
    assert!(harness.live().is_some());
    assert!(harness.press_tile().is_empty());
    assert!(harness.shortcut(TOGGLE).is_empty());
    assert!(harness.wants().every_ms.is_empty());
    harness.change_settings([(FORCED, false)]);
    assert!(harness.live().is_none() && harness.tile().unwrap().well.is_none());
}

#[test]
fn lid_switch_is_mac_only() {
    let manifest = Tested::manifest();
    let lid = manifest.settings.iter().find(|setting| setting.key == LID).expect("a lid setting");
    assert_eq!((lid.visible, &lid.default), (cfg!(target_os = "macos"), &Value::Bool(true)));
}
