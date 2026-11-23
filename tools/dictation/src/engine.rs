//! The dictation engine, on a thread of its own: it hears the key, runs the microphone, cuts long
//! recordings at pauses so the first part is written while the rest is still being said, sends
//! the parts to Groq, cleans what comes back, types it, and learns from what the user fixes.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::clean;
use crate::flac;
use crate::glossary::{Glossary, Source, from_screen};
use crate::groq::{Ask, Failure, Groq};
use crate::platform::{Cue, Hotkey, Platform, Problem, Recording, Surroundings, Tape, Typed};
use crate::sound::{self, RATE};
use crate::trigger::{Act, Key, Trigger};

const TICK: Duration = Duration::from_millis(50);
/// The meter reads this much of the newest sound.
const METER: usize = RATE / 20;
/// Parts of a long recording are at least this long, and cut at the next pause.
const PART: usize = RATE * 24;
const CUT_EVERY: Duration = Duration::from_millis(500);
/// Hands-free listening that hears nothing for this long stops by itself.
const SILENT_STOP: Duration = Duration::from_secs(10);
const LONGEST: Duration = Duration::from_secs(20 * 60);
/// The key-up that should have come, checked for after this long.
const STUCK_AFTER: Duration = Duration::from_millis(400);
/// Typing waits for the dictation key to be let go, so it doesn't turn letters into shortcuts.
const KEYS_UP_WAIT: Duration = Duration::from_millis(1500);
const WATCH_AGAIN: Duration = Duration::from_secs(3);
/// When the field is read again to see what the user corrected.
const REREAD: [Duration; 3] = [Duration::from_secs(6), Duration::from_secs(15), Duration::from_secs(40)];
const ANCHOR: usize = 40;
/// The text before the cursor that the tidy-up model sees, for context.
const CONTEXT_CHARS: usize = 240;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prefs {
    pub key: String,
    pub hotkey: Hotkey,
    pub model: String,
    pub language: String,
    pub polish: bool,
    pub learn: bool,
    pub screen: bool,
    pub sounds: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Control {
    Prefs(Box<Prefs>),
    Toggle,
    Cancel,
    Retry,
    Add(String),
    Remove(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Report {
    Listening { locked: bool, since: u64 },
    Level(f32),
    Writing,
    Idle,
    Typed { text: String, typed: bool },
    Nothing,
    Failed { says: String, retry: bool, setup: bool },
    Learned(Vec<String>),
    Glossary(Vec<(String, Source)>),
    Problem(Option<Problem>),
}

enum Input {
    Key(Key),
    Control(Control),
    Part { session: u64, index: usize, result: Result<String, Failure> },
    Edited { typed: String, edited: String },
    Quit,
}

pub struct Engine {
    input: Sender<Input>,
}

impl Engine {
    pub fn start(dir: PathBuf, platform: Box<dyn Platform>, prefs: Prefs) -> (Self, Receiver<Report>) {
        let (input, inputs) = mpsc::channel();
        let (reports, updates) = mpsc::channel();
        let glossary = Glossary::load(dir.join("dictation").join("glossary.json"));
        let mut core = Core::new(platform, prefs, glossary, input.clone(), reports);
        let _ = thread::Builder::new().name("notchbase dictation".into()).spawn(move || core.run(&inputs));
        (Self { input }, updates)
    }

    pub fn send(&self, control: Control) {
        let _ = self.input.send(Input::Control(control));
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.input.send(Input::Quit);
    }
}

struct Session {
    id: u64,
    tape: Tape,
    recording: Option<Box<dyn Recording>>,
    started: Instant,
    cut: usize,
    parts: Vec<Option<Result<String, Failure>>>,
    around: Surroundings,
    screen: Vec<String>,
    heard_speech: bool,
    checked: Instant,
}

/// What's needed to try a failed recording again.
struct Retry {
    samples: Vec<i16>,
    around: Surroundings,
    screen: Vec<String>,
}

struct Core {
    platform: Box<dyn Platform>,
    prefs: Prefs,
    groq: Groq,
    glossary: Glossary,
    trigger: Trigger,
    listening: Arc<AtomicBool>,
    live: Option<Session>,
    queue: VecDeque<Session>,
    next: u64,
    input: Sender<Input>,
    reports: Sender<Report>,
    retry: Option<Retry>,
    watching: Option<Hotkey>,
    watched_at: Option<Instant>,
    problem: Option<Problem>,
}

impl Core {
    fn new(
        platform: Box<dyn Platform>,
        prefs: Prefs,
        glossary: Glossary,
        input: Sender<Input>,
        reports: Sender<Report>,
    ) -> Self {
        let groq = Groq::new();
        groq.set_key(&prefs.key);
        Self {
            platform,
            prefs,
            groq,
            glossary,
            trigger: Trigger::default(),
            listening: Arc::default(),
            live: None,
            queue: VecDeque::new(),
            next: 1,
            input,
            reports,
            retry: None,
            watching: None,
            watched_at: None,
            problem: None,
        }
    }

    fn report(&self, report: Report) {
        let _ = self.reports.send(report);
    }

    fn run(&mut self, inputs: &Receiver<Input>) {
        self.watch();
        self.report_glossary();
        loop {
            match inputs.recv_timeout(TICK) {
                Ok(Input::Quit) | Err(RecvTimeoutError::Disconnected) => break,
                Ok(Input::Key(key)) => {
                    let act = self.trigger.see(key, Instant::now());
                    self.act(act);
                }
                Ok(Input::Control(control)) => self.control(control),
                Ok(Input::Part { session, index, result }) => self.part_done(session, index, result),
                Ok(Input::Edited { typed, edited }) => self.edited(&typed, &edited),
                Err(RecvTimeoutError::Timeout) => {}
            }
            self.tick();
        }
        self.live = None;
        self.listening.store(false, Ordering::Relaxed);
    }

    fn watch(&mut self) {
        let hotkey = self.prefs.hotkey;
        let input = self.input.clone();
        let sink = Arc::new(move |key| {
            let _ = input.send(Input::Key(key));
        });
        self.watched_at = Some(Instant::now());
        match self.platform.watch(hotkey, sink, self.listening.clone()) {
            Ok(()) => self.watching = Some(hotkey),
            Err(problem) => {
                self.watching = None;
                self.set_problem(Some(problem));
                return;
            }
        }
        self.set_problem(self.platform.problem(hotkey));
    }

    fn set_problem(&mut self, problem: Option<Problem>) {
        if problem != self.problem {
            self.problem.clone_from(&problem);
            self.report(Report::Problem(problem));
        }
    }

    fn control(&mut self, control: Control) {
        match control {
            Control::Prefs(prefs) => {
                let rewatch = prefs.hotkey != self.prefs.hotkey;
                self.groq.set_key(&prefs.key);
                self.prefs = *prefs;
                if rewatch || self.watching.is_none() {
                    self.watch();
                } else {
                    self.set_problem(self.platform.problem(self.prefs.hotkey));
                }
            }
            Control::Toggle => {
                let act = self.trigger.toggle();
                self.act(act);
            }
            Control::Cancel => {
                if self.live.is_some() {
                    self.trigger.reset();
                    self.cancel();
                }
            }
            Control::Retry => self.retry(),
            Control::Add(entry) => {
                if self.glossary.add(&entry, now()) {
                    self.glossary.save();
                    self.report_glossary();
                }
            }
            Control::Remove(text) => {
                self.glossary.remove(&text);
                self.glossary.save();
                self.report_glossary();
            }
        }
    }

    fn report_glossary(&self) {
        let mut terms: Vec<(String, Source)> =
            self.glossary.terms.iter().map(|term| (term.text.clone(), term.source)).collect();
        terms.sort_by_key(|(text, _)| text.to_lowercase());
        self.report(Report::Glossary(terms));
    }

    fn act(&mut self, act: Act) {
        match act {
            Act::Nothing => {}
            Act::Start => self.start(),
            Act::Lock => {
                if let Some(session) = &self.live {
                    self.report(Report::Listening { locked: true, since: millis(session.started) });
                }
            }
            Act::Finish => self.finish(),
            Act::Cancel => self.cancel(),
        }
    }

    fn start(&mut self) {
        if self.live.is_some() {
            return;
        }
        if !self.groq.has_key() {
            self.trigger.reset();
            self.report(Report::Failed { says: Failure::NoKey.says(), retry: false, setup: true });
            return;
        }
        if let Some(problem) = self.platform.problem(self.prefs.hotkey).filter(|problem| problem.blocks) {
            self.trigger.reset();
            self.set_problem(Some(problem.clone()));
            self.report(Report::Failed { says: problem.says, retry: false, setup: false });
            return;
        }
        let tape = Tape::default();
        let recording = match self.platform.listen(tape.clone()) {
            Ok(recording) => recording,
            Err(says) => {
                self.trigger.reset();
                self.report(Report::Failed { says, retry: false, setup: false });
                return;
            }
        };
        self.groq.warm_up();
        self.listening.store(true, Ordering::Relaxed);
        if self.prefs.sounds {
            self.platform.cue(Cue::Start);
        }
        let started = Instant::now();
        self.report(Report::Listening { locked: self.trigger.locked(), since: millis(started) });
        let around = self.platform.surroundings(self.prefs.screen);
        let screen = if self.prefs.screen { screen_terms(&around) } else { Vec::new() };
        let id = self.next;
        self.next += 1;
        self.live = Some(Session {
            id,
            tape,
            recording: Some(recording),
            started,
            cut: 0,
            parts: Vec::new(),
            around,
            screen,
            heard_speech: false,
            checked: started,
        });
    }

    fn stop_listening(&mut self) -> Option<Session> {
        let mut session = self.live.take()?;
        session.recording = None;
        self.listening.store(false, Ordering::Relaxed);
        Some(session)
    }

    fn cancel(&mut self) {
        if self.stop_listening().is_some() {
            self.report(if self.queue.is_empty() { Report::Idle } else { Report::Writing });
        }
    }

    fn finish(&mut self) {
        let Some(mut session) = self.stop_listening() else { return };
        if self.prefs.sounds {
            self.platform.cue(Cue::Stop);
        }
        let rest = session.tape.copy(session.cut, usize::MAX);
        self.write_part(&mut session, rest);
        self.queue.push_back(session);
        self.report(Report::Writing);
    }

    fn write_part(&self, session: &mut Session, samples: Vec<i16>) {
        let index = session.parts.len();
        session.parts.push(None);
        let job = Job {
            groq: self.groq.clone(),
            glossary: self.glossary.clone(),
            screen: session.screen.clone(),
            context: context(&session.around),
            model: self.prefs.model.clone(),
            language: self.prefs.language.clone(),
            polish: self.prefs.polish,
        };
        let (input, id) = (self.input.clone(), session.id);
        let _ = thread::Builder::new().name("notchbase dictation part".into()).spawn(move || {
            let result = job.write(&samples);
            let _ = input.send(Input::Part { session: id, index, result });
        });
    }

    fn part_done(&mut self, id: u64, index: usize, result: Result<String, Failure>) {
        let session = self.live.iter_mut().chain(self.queue.iter_mut()).find(|session| session.id == id);
        if let Some(part) = session.and_then(|session| session.parts.get_mut(index)) {
            *part = Some(result);
        }
        self.deliver();
    }

    fn deliver(&mut self) {
        while self.queue.front().is_some_and(|session| session.parts.iter().all(Option::is_some)) {
            let Some(session) = self.queue.pop_front() else { break };
            self.deliver_one(session);
        }
        if self.queue.is_empty() && self.live.is_none() {
            self.report(Report::Idle);
        }
    }

    fn deliver_one(&mut self, session: Session) {
        let mut texts = Vec::new();
        for part in &session.parts {
            match part {
                Some(Ok(text)) => texts.push(text.clone()),
                Some(Err(failure)) => {
                    let setup = matches!(failure, Failure::NoKey | Failure::BadKey);
                    self.report(Report::Failed { says: failure.says(), retry: failure.passes(), setup });
                    if failure.passes() {
                        let samples = session.tape.all();
                        self.retry = Some(Retry { samples, around: session.around, screen: session.screen });
                    }
                    return;
                }
                None => return,
            }
        }
        let text = clean::joined(&texts);
        if text.is_empty() {
            self.report(Report::Nothing);
            return;
        }
        self.type_out(&text, &session.around);
        if !self.prefs.learn {
            return;
        }
        let learned = self.glossary.heard(&text, &session.screen, now());
        self.glossary.save();
        if !learned.is_empty() {
            self.report(Report::Learned(learned));
            self.report_glossary();
        }
    }

    fn type_out(&mut self, text: &str, around: &Surroundings) {
        let fitted = clean::fitted(text, around.before.as_deref(), around.after.as_deref());
        self.wait_for_keys_up();
        match self.platform.type_text(&fitted, self.prefs.learn && around.before.is_some()) {
            Ok(typed) => {
                self.report(Report::Typed { text: text.to_owned(), typed: true });
                if self.prefs.learn {
                    self.watch_for_edits(typed, &fitted, around);
                }
            }
            Err(_) => self.report(Report::Typed { text: text.to_owned(), typed: false }),
        }
    }

    fn wait_for_keys_up(&self) {
        let until = Instant::now() + KEYS_UP_WAIT;
        while self.platform.hotkey_down(self.prefs.hotkey) == Some(true) && Instant::now() < until {
            thread::sleep(Duration::from_millis(15));
        }
    }

    fn watch_for_edits(&self, typed: Typed, fitted: &str, around: &Surroundings) {
        let Some(mut reread) = typed.reread else { return };
        let before = tail(around.before.as_deref().unwrap_or(""), ANCHOR);
        let after = head(around.after.as_deref().unwrap_or(""), ANCHOR);
        let typed_text = fitted.trim().to_owned();
        let input = self.input.clone();
        let _ = thread::Builder::new().name("notchbase dictation learning".into()).spawn(move || {
            let mut last = None;
            let mut slept = Duration::ZERO;
            for at in REREAD {
                thread::sleep(at - slept);
                slept = at;
                if let Some(edited) = reread().and_then(|value| region(&value, &before, &after, &typed_text)) {
                    last = Some(edited);
                }
            }
            if let Some(edited) = last.filter(|edited| *edited != typed_text) {
                let _ = input.send(Input::Edited { typed: typed_text, edited });
            }
        });
    }

    fn edited(&mut self, typed: &str, edited: &str) {
        let learned = self.glossary.corrected(typed, edited, now());
        self.glossary.save();
        if !learned.is_empty() {
            self.report(Report::Learned(learned));
            self.report_glossary();
        }
    }

    fn retry(&mut self) {
        let Some(retry) = self.retry.take() else { return };
        let id = self.next;
        self.next += 1;
        let mut session = Session {
            id,
            tape: Tape::default(),
            recording: None,
            started: Instant::now(),
            cut: 0,
            parts: Vec::new(),
            around: Surroundings { before: None, after: None, ..retry.around },
            screen: retry.screen,
            heard_speech: true,
            checked: Instant::now(),
        };
        session.tape.push(&retry.samples);
        self.write_part(&mut session, retry.samples);
        self.queue.push_back(session);
        self.report(Report::Writing);
    }

    fn tick(&mut self) {
        let now = Instant::now();
        if self.watching != Some(self.prefs.hotkey) && self.watched_at.is_none_or(|at| at.elapsed() >= WATCH_AGAIN) {
            self.watch();
        }
        let Some(session) = &self.live else { return };
        let elapsed = session.started.elapsed();
        self.report(Report::Level(sound::level(&session.tape.tail(METER))));
        if self.trigger.held() && elapsed >= STUCK_AFTER && self.platform.hotkey_down(self.prefs.hotkey) == Some(false)
        {
            let act = self.trigger.released(now);
            self.act(act);
            return;
        }
        if elapsed >= LONGEST {
            self.trigger.reset();
            self.finish();
            return;
        }
        if session.checked.elapsed() >= CUT_EVERY {
            self.check_live();
        }
    }

    /// Notices speech, stops hands-free listening that hears none, and cuts long recordings.
    fn check_live(&mut self) {
        let Some(mut session) = self.live.take() else { return };
        session.checked = Instant::now();
        let length = session.tape.count();
        if !session.heard_speech {
            session.heard_speech = sound::speech(&session.tape.all()).is_some();
            if !session.heard_speech && self.trigger.locked() && session.started.elapsed() >= SILENT_STOP {
                self.live = Some(session);
                self.trigger.reset();
                self.cancel();
                self.report(Report::Nothing);
                return;
            }
        }
        if length - session.cut >= PART {
            let pending = session.tape.copy(session.cut, length);
            if let Some(at) = sound::pause_after(&pending, PART - RATE * 4) {
                let part = pending[..at].to_vec();
                session.cut += at;
                self.write_part(&mut session, part);
            }
        }
        self.live = Some(session);
    }
}

/// One part of a recording on its way to text.
struct Job {
    groq: Groq,
    glossary: Glossary,
    screen: Vec<String>,
    context: String,
    model: String,
    language: String,
    polish: bool,
}

impl Job {
    fn write(&self, samples: &[i16]) -> Result<String, Failure> {
        let Some(speech) = sound::speech(samples) else { return Ok(String::new()) };
        let prepared = sound::prepared(samples, speech);
        let audio = flac::encode(&prepared).map_err(Failure::Server)?;
        let ask = Ask {
            model: self.model.clone(),
            language: self.language.clone(),
            prompt: self.glossary.prompt(&self.screen),
            seconds: prepared.len() as f32 / RATE as f32,
        };
        let heard = self.groq.transcribe(&audio, &ask)?;
        let text = clean::heard(&heard.text, &heard.segments, speech.voiced);
        let text = self.glossary.fix(&text, &self.screen);
        if !self.polish || text.is_empty() {
            return Ok(text);
        }
        let terms = self.glossary.for_polish(&self.screen);
        let polished = clean::polished(&text, self.groq.polish(&text, &terms, &self.context));
        Ok(self.glossary.fix(&polished, &self.screen))
    }
}

fn screen_terms(around: &Surroundings) -> Vec<String> {
    let mut text = around.visible.clone();
    for part in [&around.before, &around.after].into_iter().flatten() {
        text.push('\n');
        text.push_str(part);
    }
    from_screen(&text)
}

fn context(around: &Surroundings) -> String {
    tail(around.before.as_deref().unwrap_or(""), CONTEXT_CHARS)
}

fn tail(text: &str, chars: usize) -> String {
    let count = text.chars().count();
    text.chars().skip(count.saturating_sub(chars)).collect()
}

fn head(text: &str, chars: usize) -> String {
    text.chars().take(chars).collect()
}

/// What now stands where the words were typed: between the text that was before the cursor and
/// the text that was after it.
fn region(value: &str, before: &str, after: &str, typed: &str) -> Option<String> {
    let start = if before.is_empty() { 0 } else { value.find(before)? + before.len() };
    let rest = &value[start..];
    let end = if after.is_empty() { rest.len() } else { rest.find(after)? };
    let found = rest[..end].trim();
    (found.len() <= typed.len() * 2 + ANCHOR).then(|| found.to_owned())
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_secs())
}

/// An `Instant` as milliseconds since the Unix epoch, for the notch's timers.
fn millis(at: Instant) -> u64 {
    let wall = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let ago = at.elapsed();
    wall.saturating_sub(ago).as_millis() as u64
}

#[cfg(test)]
mod tests;
