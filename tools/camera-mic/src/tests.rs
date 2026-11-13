use notchbase_sdk::protocol::{Node, Request};
use notchbase_sdk::testing::Harness;

use super::*;

fn call() -> Sensors {
    Sensors { camera: true, microphone: true, apps: vec!["Zoom".into()], muted: false }
}

fn dots(harness: &mut Harness<CameraMic>) -> Vec<Color> {
    let Some(Node::Row { children, .. }) = harness.live().map(|live| live.leading) else { return Vec::new() };
    children
        .iter()
        .filter_map(|node| match node {
            Node::Dot { color, .. } => Some(*color),
            _ => None,
        })
        .collect()
}

#[test]
fn nothing_shows_while_off() {
    let mut harness = Harness::<CameraMic>::start();
    assert!(harness.live().is_none());
    harness.sensors(Sensors::default());
    assert!(harness.live().is_none() && harness.tile().is_none());
}

#[test]
fn call_shows_dots_and_app() {
    let mut harness = Harness::<CameraMic>::start();
    harness.sensors(call());
    assert_eq!(dots(&mut harness), [Tone::Green.color(), Tone::Orange.color()]);
    assert!(harness.shows("Zoom"));
    harness.sensors(Sensors { camera: false, ..call() });
    assert_eq!(dots(&mut harness), [Tone::Orange.color()]);
    harness.sensors(Sensors::default());
    assert!(harness.live().is_none());
}

#[test]
fn unknown_app_is_named_plainly() {
    let mut harness = Harness::<CameraMic>::start();
    harness.sensors(Sensors { camera: true, ..Sensors::default() });
    assert!(harness.shows(CAMERA_ON));
    assert!(harness.live().unwrap().controls.is_some_and(|controls| {
        !controls.any(&|node| matches!(node, Node::Button { icon: Some(Icon::Microphone | Icon::MicrophoneMuted), .. }))
    }));
    harness.sensors(Sensors {
        camera: true,
        microphone: true,
        apps: vec!["FaceTime".into(), "Zoom".into()],
        muted: false,
    });
    assert!(harness.shows("FaceTime +1"));
}

#[test]
fn mic_button_toggles_mute() {
    let mut harness = Harness::<CameraMic>::start();
    harness.sensors(call());
    assert_eq!(harness.click_icon(Icon::Microphone, 0), [Request::MuteMicrophone { muted: true }]);
    assert!(harness.shows(MUTED), "the button answers before the system reports back");
    harness.sensors(Sensors { muted: true, ..call() });
    assert_eq!(harness.click_icon(Icon::MicrophoneMuted, 0), [Request::MuteMicrophone { muted: false }]);
    assert!(!harness.shows(MUTED));
}

#[test]
fn hiding_lasts_until_both_off() {
    let mut harness = Harness::<CameraMic>::start();
    harness.sensors(call());
    harness.click_icon(Icon::Xmark, 0);
    assert!(harness.live().is_none());
    harness.sensors(Sensors { camera: false, ..call() });
    assert!(harness.live().is_none(), "the same call stays hidden");
    harness.sensors(Sensors::default());
    harness.sensors(call());
    assert!(harness.live().is_some(), "the next call shows again");
}

#[test]
fn each_indicator_can_be_off() {
    let mut harness = Harness::<CameraMic>::start_with([(MICROPHONE, false)]);
    harness.sensors(Sensors { microphone: true, apps: vec!["Dictation".into()], ..Sensors::default() });
    assert!(harness.live().is_none());
    harness.sensors(call());
    assert_eq!(dots(&mut harness), [Tone::Green.color()]);
    harness.change_settings([(CAMERA, false)]);
    assert!(harness.live().is_none());
}

#[test]
fn long_app_names_are_cut_short() {
    assert_eq!(shortened("Zoom"), "Zoom");
    assert_eq!(shortened("Microsoft Teams (work or school)"), "Microsoft Teams (w\u{2026}");
}
