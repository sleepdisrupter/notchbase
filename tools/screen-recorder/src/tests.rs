use notchbase_sdk::protocol::{Event, Node, Request, Start, Values};
use notchbase_sdk::{Driver, Runtime, Waker};

use super::*;

fn started() -> Runtime<ScreenRecorder> {
    let mut runtime = Runtime::new(Waker::noop());
    runtime.start(Start::default());
    runtime
}

fn state(runtime: &mut Runtime<ScreenRecorder>, state: RecordingState) {
    runtime.handle(vec![Event::Recording { update: RecordingUpdate::State { state } }]);
}

fn microphones() -> Vec<AudioInput> {
    let input = |id: &str, is_default| AudioInput { id: id.into(), name: format!("{id} name"), is_default };
    vec![input("a", false), input("b", true)]
}

#[test]
fn shortcut_follows_the_state() {
    let mut runtime = started();
    let press =
        |runtime: &mut Runtime<ScreenRecorder>| runtime.handle(vec![Event::Shortcut { id: RECORD.into() }]).requests;
    assert!(matches!(press(&mut runtime)[..], [Request::StartRecording { .. }]));
    state(&mut runtime, RecordingState::Choosing);
    assert_eq!(press(&mut runtime), vec![Request::CancelCapture]);
    state(&mut runtime, RecordingState::Recording { since: 5 });
    assert_eq!(press(&mut runtime), vec![Request::StopRecording]);
    state(&mut runtime, RecordingState::Saving);
    assert!(press(&mut runtime).is_empty());
}

#[test]
fn recording_shows_live_timer() {
    let mut runtime = started();
    assert!(runtime.views().live.is_none());
    state(&mut runtime, RecordingState::Recording { since: 1_000 });
    let views = runtime.views();
    let live = views.live.expect("a live activity");
    assert!(matches!(live.trailing, Node::Timer { at: 1_000, .. }));
    let tile = views.tile.unwrap();
    assert_eq!((tile.title.as_str(), tile.tint), ("Recording", Some(Tone::Red)));
}

fn microphone_menu(runtime: &mut Runtime<ScreenRecorder>) -> Node {
    let footer = runtime.views().tile.unwrap().footer.expect("audio controls");
    let Node::Row { children, .. } = footer else { panic!("{footer:?}") };
    children.into_iter().next().expect("a microphone menu")
}

fn choose(runtime: &mut Runtime<ScreenRecorder>, index: usize) -> Vec<Request> {
    let Node::Dropdown { on_select: Some(handler), .. } = microphone_menu(runtime) else { panic!() };
    let value = notchbase_sdk::protocol::UiValue::Index(index);
    let surface = notchbase_sdk::protocol::Surface::Tile;
    runtime.handle(vec![Event::Ui { surface, handler, value }]).requests
}

#[test]
fn menu_turns_mic_on_and_picks() {
    let mut runtime = started();
    runtime.handle(vec![Event::AudioInputs { inputs: microphones() }]);
    let Node::Dropdown { options, selected, compact, .. } = microphone_menu(&mut runtime) else { panic!() };
    let labels: Vec<&str> = options.iter().map(|choice| choice.label.as_str()).collect();
    assert_eq!((labels, selected, compact), (vec![NO_MICROPHONE, "a name", "b name"], None, true));

    let requests = choose(&mut runtime, 1);
    let keys: Vec<&str> = requests
        .iter()
        .filter_map(|request| match request {
            Request::SaveSetting { key, .. } => Some(key.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(keys, [options::MICROPHONE, options::MICROPHONE_ID]);
    let Node::Dropdown { selected, .. } = microphone_menu(&mut runtime) else { panic!() };
    assert_eq!(selected, Some(1));

    choose(&mut runtime, MICROPHONE_OFF);
    let Node::Dropdown { selected, .. } = microphone_menu(&mut runtime) else { panic!() };
    assert_eq!(selected, None, "off again");
}

#[test]
fn unchosen_mic_shows_default() {
    let mut runtime = started();
    runtime.send(Msg::Microphone(true));
    runtime.handle(vec![Event::AudioInputs { inputs: microphones() }]);
    let Node::Dropdown { selected, .. } = microphone_menu(&mut runtime) else { panic!() };
    assert_eq!(selected, Some(2));
}

#[test]
fn tile_shares_a_row() {
    let mut runtime = started();
    assert!(!runtime.views().tile.unwrap().wide);
}

#[test]
fn saved_recording_is_shelved() {
    let mut runtime = started();
    let update = RecordingUpdate::Saved { path: "clip.mp4".into(), seconds: 42.4, note: None };
    let requests = runtime.handle(vec![Event::Recording { update }]).requests;
    assert_eq!(requests[0], Request::Shelve { paths: vec!["clip.mp4".into()] });
    assert!(matches!(requests[1], Request::Notice { .. }));
}

#[test]
fn settings_decide_options() {
    let mut runtime = Runtime::<ScreenRecorder>::new(Waker::noop());
    let mut settings = Values::new();
    settings.insert("frame_rate".into(), "30".into());
    settings.insert("dim_outside".into(), false.into());
    runtime.start(Start { settings, ..Start::default() });
    let [Request::StartRecording { options }] = &runtime.send(Msg::Toggle).requests[..] else { panic!() };
    assert_eq!((options.max_fps, options.dim_outside), (Some(30), false));
}

#[test]
fn live_activity_stops_it() {
    let mut recorder = notchbase_sdk::testing::Harness::<ScreenRecorder>::start();
    recorder
        .event(Event::Recording { update: RecordingUpdate::State { state: RecordingState::Recording { since: 5 } } });
    assert_eq!(recorder.click_icon(Icon::Stop, 0), [Request::StopRecording]);
}
