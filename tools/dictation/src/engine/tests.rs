use std::sync::Mutex;

use super::*;
use crate::platform::KeySink;

#[derive(Default)]
struct Shared {
    keys: Option<KeySink>,
    tape: Option<Tape>,
    typed: Vec<String>,
    listening: Option<Arc<AtomicBool>>,
    refuse_watch: bool,
}

#[derive(Clone, Default)]
struct Fake(Arc<Mutex<Shared>>);

struct Mic;

impl Recording for Mic {}

impl Platform for Fake {
    fn watch(&mut self, _hotkey: Hotkey, keys: KeySink, listening: Arc<AtomicBool>) -> Result<(), Problem> {
        let mut shared = self.0.lock().unwrap();
        if shared.refuse_watch {
            return Err(Problem { says: "Allow Accessibility".into(), fix: None, button: "Open", blocks: false });
        }
        shared.keys = Some(keys);
        shared.listening = Some(listening);
        Ok(())
    }

    fn hotkey_down(&self, _hotkey: Hotkey) -> Option<bool> {
        None
    }

    fn listen(&mut self, tape: Tape) -> Result<Box<dyn Recording>, String> {
        self.0.lock().unwrap().tape = Some(tape);
        Ok(Box::new(Mic))
    }

    fn surroundings(&self, _read_text: bool) -> Surroundings {
        Surroundings { app: "Notes".into(), visible: "Notes about Kubernetes".into(), ..Surroundings::default() }
    }

    fn type_text(&self, text: &str, _reread: bool) -> Result<Typed, String> {
        self.0.lock().unwrap().typed.push(text.to_owned());
        Ok(Typed { reread: None })
    }

    fn cue(&self, _cue: Cue) {}

    fn problem(&self, _hotkey: Hotkey) -> Option<Problem> {
        None
    }
}

fn prefs(key: &str) -> Prefs {
    Prefs {
        key: key.into(),
        hotkey: Hotkey::Fn,
        model: crate::groq::ACCURATE.into(),
        language: "en".into(),
        polish: false,
        learn: true,
        screen: true,
        sounds: false,
    }
}

fn started(key: &str) -> (Engine, Receiver<Report>, Fake, PathBuf) {
    let dir =
        std::env::temp_dir().join(format!("notchbase-dictation-engine-{}-{:?}", std::process::id(), Instant::now()));
    let fake = Fake::default();
    let (engine, reports) = Engine::start(dir.clone(), Box::new(fake.clone()), prefs(key));
    (engine, reports, fake, dir)
}

fn press(fake: &Fake, key: Key) {
    let sink = fake.0.lock().unwrap().keys.clone();
    sink.expect("the key is watched")(key);
}

fn until(reports: &Receiver<Report>, wanted: impl Fn(&Report) -> bool) -> Report {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if let Ok(report) = reports.recv_timeout(Duration::from_millis(100))
            && wanted(&report)
        {
            return report;
        }
    }
    panic!("no such report came");
}

fn wait_for_watch(fake: &Fake) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while fake.0.lock().unwrap().keys.is_none() {
        assert!(Instant::now() < deadline, "the key is never watched");
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn no_key_asks_for_setup() {
    let (_engine, reports, fake, dir) = started("");
    wait_for_watch(&fake);
    press(&fake, Key::Down);
    let report = until(&reports, |report| matches!(report, Report::Failed { .. }));
    assert_eq!(report, Report::Failed { says: Failure::NoKey.says(), retry: false, setup: true });
    assert!(fake.0.lock().unwrap().tape.is_none(), "the microphone stays off");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn silence_types_nothing() {
    let (_engine, reports, fake, dir) = started("gsk_test");
    wait_for_watch(&fake);
    press(&fake, Key::Down);
    until(&reports, |report| matches!(report, Report::Listening { locked: false, .. }));
    let listening = fake.0.lock().unwrap().listening.clone().unwrap();
    assert!(listening.load(Ordering::Relaxed), "Escape is kept from the app while listening");
    fake.0.lock().unwrap().tape.clone().unwrap().push(&vec![0; RATE]);
    thread::sleep(TAP_AND_A_BIT);
    press(&fake, Key::Up);
    until(&reports, |report| *report == Report::Nothing);
    until(&reports, |report| *report == Report::Idle);
    assert!(fake.0.lock().unwrap().typed.is_empty());
    assert!(!listening.load(Ordering::Relaxed));
    let _ = std::fs::remove_dir_all(dir);
}

const TAP_AND_A_BIT: Duration = Duration::from_millis(crate::trigger::TAP.as_millis() as u64 + 30);

#[test]
fn double_tap_locks_and_escape_cancels() {
    let (_engine, reports, fake, dir) = started("gsk_test");
    wait_for_watch(&fake);
    press(&fake, Key::Down);
    press(&fake, Key::Up);
    press(&fake, Key::Down);
    press(&fake, Key::Up);
    until(&reports, |report| matches!(report, Report::Listening { locked: true, .. }));
    press(&fake, Key::Escape);
    until(&reports, |report| *report == Report::Idle);
    assert!(fake.0.lock().unwrap().typed.is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn single_tap_is_dropped() {
    let (_engine, reports, fake, dir) = started("gsk_test");
    wait_for_watch(&fake);
    press(&fake, Key::Down);
    press(&fake, Key::Up);
    until(&reports, |report| *report == Report::Idle);
    assert!(fake.0.lock().unwrap().typed.is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn glossary_changes_are_reported() {
    let (engine, reports, _fake, dir) = started("gsk_test");
    engine.send(Control::Add("brock => Groq".into()));
    let report = until(&reports, |report| matches!(report, Report::Glossary(terms) if !terms.is_empty()));
    assert_eq!(report, Report::Glossary(vec![("Groq".into(), Source::Added)]));
    engine.send(Control::Remove("Groq".into()));
    until(&reports, |report| matches!(report, Report::Glossary(terms) if terms.is_empty()));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn refused_watch_is_a_problem() {
    let fake = Fake::default();
    fake.0.lock().unwrap().refuse_watch = true;
    let dir = std::env::temp_dir().join(format!("notchbase-dictation-refused-{}", std::process::id()));
    let (_engine, reports) = Engine::start(dir.clone(), Box::new(fake), prefs("gsk_test"));
    let report = until(&reports, |report| matches!(report, Report::Problem(Some(_))));
    let Report::Problem(Some(problem)) = report else { unreachable!() };
    assert_eq!(problem.says, "Allow Accessibility");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn region_finds_the_edited_words() {
    let typed = "ask Brock about it";
    assert_eq!(region("Hi, ask Groq about it. Bye", "Hi,", ". Bye", typed).as_deref(), Some("ask Groq about it"));
    assert_eq!(region("ask Groq about it", "", "", typed).as_deref(), Some("ask Groq about it"));
    assert_eq!(region("something else entirely", "Hi,", "", typed), None, "the text around it is gone");
    let long = "x".repeat(500);
    assert_eq!(region(&long, "", "", typed), None, "too much changed to learn from");
}

#[test]
fn ends_of_text_count_characters() {
    assert_eq!(tail("héllo", 3), "llo");
    assert_eq!(head("héllo", 2), "hé");
    assert_eq!(tail("hi", 10), "hi");
}

#[test]
#[ignore = "sends the WAV in NOTCHBASE_DICTATION_WAV to Groq with the key in GROQ_API_KEY"]
fn real_speech_is_written() {
    let wav = std::fs::read(std::env::var("NOTCHBASE_DICTATION_WAV").unwrap()).unwrap();
    let samples: Vec<i16> = wav[44..].as_chunks::<2>().0.iter().map(|pair| i16::from_le_bytes(*pair)).collect();
    let groq = Groq::new();
    groq.set_key(&std::env::var("GROQ_API_KEY").unwrap());
    let mut glossary = Glossary::default();
    for term in ["notchbase", "Groq", "Dana", "Priya", "Terraform", "GitHub"] {
        glossary.add(term, 1);
    }
    for polish in [false, true] {
        let job = Job {
            groq: groq.clone(),
            glossary: glossary.clone(),
            screen: vec!["Roam".into(), "Supabase".into()],
            context: "Notes for today:".into(),
            model: crate::groq::ACCURATE.into(),
            language: "en".into(),
            polish,
        };
        let started = Instant::now();
        let text = job.write(&samples).expect("Groq writes it");
        eprintln!("polish {polish}: {:?} in {:?}", text, started.elapsed());
        assert!(!text.is_empty());
    }
}
