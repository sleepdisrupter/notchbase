use notchbase_sdk::protocol::Request;
use notchbase_sdk::testing::Harness;

use super::*;

struct Off;

impl Backend for Off {
    fn start(_context: &Context, _prefs: Prefs) -> Option<(Engine, Receiver<Report>)> {
        None
    }
}

type Subject = Dictation<Off>;

fn report(harness: &mut Harness<Subject>, report: Report) {
    harness.send(Msg::Report(Box::new(report)));
}

#[test]
fn without_a_key_the_page_explains_setup() {
    let mut harness = Harness::<Subject>::start();
    assert!(harness.shows("Dictation uses Groq's Whisper with your own free key"));
    assert!(harness.shows("Add a Groq key to start"));
    harness.type_text("gsk_…", "  gsk_abc  ");
    harness.click("Save");
    assert_eq!(harness.saved(KEY), Some(&Value::String("gsk_abc".into())));
}

#[test]
fn listening_shows_a_meter_and_controls() {
    let mut harness = Harness::<Subject>::start_with([(KEY, "gsk_abc")]);
    assert!(harness.live().is_none());
    report(&mut harness, Report::Listening { locked: false, since: 1 });
    for level in [0.2, 0.9, 0.4] {
        report(&mut harness, Report::Level(level));
    }
    let live = harness.live().expect("a live activity while listening");
    assert!(live.controls.is_some());
    assert_eq!(harness.model().levels.len(), 3);
    report(&mut harness, Report::Writing);
    assert!(harness.shows("Writing"));
    report(&mut harness, Report::Idle);
    assert!(harness.live().is_none());
}

#[test]
fn failures_offer_the_fix() {
    let mut harness = Harness::<Subject>::start();
    report(&mut harness, Report::Failed { says: "Add your Groq key to dictate".into(), retry: false, setup: true });
    assert!(harness.noticed().iter().any(|text| text.contains("Add your Groq key")));
    let requests = harness.click_notice("Set up").to_vec();
    assert!(requests.iter().any(|request| matches!(request, Request::OpenPage)), "{requests:?}");
}

#[test]
fn untyped_words_are_copied() {
    let mut harness = Harness::<Subject>::start_with([(KEY, "gsk_abc")]);
    report(&mut harness, Report::Typed { text: "hello there".into(), typed: false });
    let requests = harness.requests().to_vec();
    assert!(requests.iter().any(|request| matches!(request, Request::CopyText { text } if text == "hello there")));
    assert!(harness.shows("hello there"), "it's in the recent list");
    let copied = harness.click("Copy").to_vec();
    assert!(copied.iter().any(|request| matches!(request, Request::CopyText { .. })));
}

#[test]
fn glossary_is_listed_and_editable() {
    let mut harness = Harness::<Subject>::start_with([(KEY, "gsk_abc")]);
    report(
        &mut harness,
        Report::Glossary(vec![("notchbase".into(), Source::Added), ("Groq".into(), Source::Corrected)]),
    );
    assert!(harness.shows("notchbase") && harness.shows("Groq"));
    report(&mut harness, Report::Learned(vec!["Groq".into()]));
    assert!(harness.noticed().iter().any(|text| text.contains("Learned Groq")));
}

#[test]
fn hotkey_follows_the_platform() {
    let manifest = <Subject as Tool>::manifest();
    let hotkey = manifest.settings.iter().find(|setting| setting.key == HOTKEY).unwrap();
    assert_eq!(hotkey.default, Value::String(if cfg!(windows) { "ctrl_win" } else { "fn" }.into()));
    assert_eq!(Hotkey::named("nonsense"), Hotkey::default());
}
