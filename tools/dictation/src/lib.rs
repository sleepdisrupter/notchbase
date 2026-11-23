//! Dictation: hold fn (Ctrl+Win on Windows) and talk, or double-tap it to keep listening and
//! type as you talk.
//! Groq's Whisper writes it down with the user's own key, a quick model tidies it, and the
//! words are typed where the cursor is. It learns the user's terms as they go: the ones they
//! add, the ones they correct, the ones they keep saying and the ones on screen.

mod clean;
mod diff;
pub mod engine;
mod flac;
mod glossary;
mod groq;
mod log;
pub mod platform;
mod sound;
mod trigger;
mod words;

use std::collections::VecDeque;
use std::marker::PhantomData;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use notchbase_sdk::column;
use notchbase_sdk::prelude::*;

use engine::{Control, Engine, Prefs, Report};
use glossary::Source;
use platform::{Hotkey, Problem};

pub const ID: &str = "dictation";
const TOGGLE: &str = "toggle";
const KEY: &str = "api_key";
const HOTKEY: &str = "hotkey";
const MODEL: &str = "model";
const LANGUAGE: &str = "language";
const POLISH: &str = "polish";
const LEARN: &str = "learn";
const SCREEN: &str = "screen";
const SOUNDS: &str = "sounds";
const KEYS_PAGE: &str = "https://console.groq.com/keys";
const LANGUAGES: [(&str, &str); 18] = [
    ("auto", "Detect"),
    ("en", "English"),
    ("es", "Spanish"),
    ("fr", "French"),
    ("de", "German"),
    ("it", "Italian"),
    ("pt", "Portuguese"),
    ("nl", "Dutch"),
    ("pl", "Polish"),
    ("ru", "Russian"),
    ("uk", "Ukrainian"),
    ("tr", "Turkish"),
    ("sv", "Swedish"),
    ("ja", "Japanese"),
    ("zh", "Chinese"),
    ("ko", "Korean"),
    ("hi", "Hindi"),
    ("ar", "Arabic"),
];
const BARS: usize = 7;
const BAR_WIDTH: f32 = 2.6;
const BAR_GAP: f32 = 2.0;
const BARS_HEIGHT: f32 = 14.0;
const GLYPH: f32 = 13.0;
const HISTORY: usize = 8;
const SHOWN_TERMS: usize = 24;
const NOTICE_FOR: Duration = Duration::from_secs(5);
const LEARNED_FOR: Duration = Duration::from_millis(2600);

/// What starts the engine: the real one, or none in tests.
pub trait Backend: 'static {
    fn start(context: &Context, prefs: Prefs) -> Option<(Engine, Receiver<Report>)>;
}

pub struct Native;

impl Backend for Native {
    fn start(context: &Context, prefs: Prefs) -> Option<(Engine, Receiver<Report>)> {
        Some(Engine::start(context.data_dir(), platform::native(), prefs))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Idle,
    Listening { locked: bool, since: u64 },
    Writing,
}

pub struct Dictation<B: Backend = Native> {
    engine: Option<Engine>,
    prefs: Prefs,
    phase: Phase,
    levels: VecDeque<f32>,
    history: VecDeque<String>,
    glossary: Vec<(String, Source)>,
    problem: Option<Problem>,
    draft_key: String,
    draft_term: String,
    backend: PhantomData<B>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    Report(Box<Report>),
    Configured(Settings),
    Toggle,
    Cancel,
    Retry,
    Setup,
    Fix,
    GetKey,
    DraftKey(String),
    SaveKey,
    DraftTerm(String),
    AddTerm,
    RemoveTerm(String),
    Copy(usize),
}

fn prefs(settings: &Settings) -> Prefs {
    let model = if settings.text(MODEL) == "fast" { groq::TURBO } else { groq::ACCURATE };
    let language = settings.text(LANGUAGE);
    Prefs {
        key: settings.text(KEY).trim().to_owned(),
        hotkey: Hotkey::named(settings.text(HOTKEY)),
        model: model.to_owned(),
        language: if language == "auto" { String::new() } else { language.to_owned() },
        polish: settings.bool(POLISH),
        learn: settings.bool(LEARN),
        screen: settings.bool(SCREEN),
        sounds: settings.bool(SOUNDS),
    }
}

impl<B: Backend> Dictation<B> {
    fn control(&self, control: Control) {
        if let Some(engine) = &self.engine {
            engine.send(control);
        }
    }

    fn reported(&mut self, report: Report) -> Command<Msg> {
        match report {
            Report::Listening { locked, since } => {
                if !matches!(self.phase, Phase::Listening { .. }) {
                    self.levels.clear();
                }
                self.phase = Phase::Listening { locked, since };
                Command::none()
            }
            Report::Level(level) => {
                self.levels.push_back(level);
                while self.levels.len() > BARS {
                    self.levels.pop_front();
                }
                Command::none()
            }
            Report::Writing => {
                self.phase = Phase::Writing;
                Command::none()
            }
            Report::Idle => {
                self.phase = Phase::Idle;
                Command::none()
            }
            Report::Typed { text, typed } => {
                self.history.push_front(text.clone());
                self.history.truncate(HISTORY);
                if typed {
                    Command::none()
                } else {
                    Command::copy_text(text).and(Command::notify(if cfg!(windows) {
                        "No text field. Copied, paste with Ctrl+V"
                    } else {
                        "No text field. Copied, paste with ⌘V"
                    }))
                }
            }
            Report::Nothing => Command::notify("Didn't catch that"),
            Report::Failed { says, retry, setup } => {
                let shown = Notice::new(icon(Icon::MicrophoneMuted).size(GLYPH).tone(Tone::Red), text(&says).callout())
                    .pop()
                    .duration(NOTICE_FOR);
                if setup {
                    Command::notice_with_controls(shown, button("Set up").on_press(Msg::Setup))
                } else if retry {
                    Command::notice_with_controls(shown, button("Retry").on_press(Msg::Retry))
                } else {
                    Command::notice(shown)
                }
            }
            Report::Learned(words) => {
                let learned = format!("Learned {}", words.join(", "));
                let shown = Notice::new(icon(Icon::Spark).size(GLYPH).tone(Tone::Yellow), text(learned).callout())
                    .duration(LEARNED_FOR);
                Command::notice(shown)
            }
            Report::Glossary(terms) => {
                self.glossary = terms;
                Command::none()
            }
            Report::Problem(problem) => {
                let shown = problem.clone().filter(|problem| problem.fix.is_some()).map(|problem| {
                    let notice =
                        Notice::new(icon(Icon::Exclamation).size(GLYPH).tone(Tone::Orange), text(&problem.says))
                            .banner()
                            .duration(NOTICE_FOR * 2);
                    Command::notice_with_controls(notice, button(problem.button).on_press(Msg::Fix))
                });
                self.problem = problem;
                shown.unwrap_or_else(Command::none)
            }
        }
    }

    fn bars(&self) -> Element<Msg> {
        let width = BARS as f32 * (BAR_WIDTH + BAR_GAP) - BAR_GAP;
        let padded =
            std::iter::repeat_n(0.0, BARS.saturating_sub(self.levels.len())).chain(self.levels.iter().copied());
        let shapes = padded.enumerate().map(|(i, level)| {
            let height = (3.0 + level * (BARS_HEIGHT - 3.0)).min(BARS_HEIGHT);
            let x = i as f32 * (BAR_WIDTH + BAR_GAP);
            draw::rect(x, (BARS_HEIGHT - height) / 2.0, BAR_WIDTH, height)
                .radius(BAR_WIDTH / 2.0)
                .fill(Color::white(0.9))
        });
        canvas(width, BARS_HEIGHT).extend(shapes).into()
    }

    fn setup_card(&self) -> Element<Msg> {
        let steps = column![
            text("Dictation uses Groq's Whisper with your own free key").headline(),
            text("1. Open console.groq.com/keys and sign in").secondary(),
            text("2. Press Create API Key and copy it").secondary(),
            text("3. Paste it here").secondary(),
        ]
        .spacing(4.0);
        let field = row![
            text_input(&self.draft_key).placeholder("gsk_…").on_change(Msg::DraftKey).on_submit(Msg::SaveKey),
            button("Save").filled().on_press(Msg::SaveKey).enabled(!self.draft_key.trim().is_empty()),
        ]
        .spacing(8.0);
        let open = button("Open Groq").icon(Icon::Link).on_press(Msg::GetKey);
        card(column![steps, field, row![open, spacer()]].spacing(10.0))
    }

    fn how_to(&self) -> Element<Msg> {
        let caps = self.prefs.hotkey.caps().to_vec();
        let state = match self.phase {
            Phase::Idle => text("Hold to talk, double-tap to type as you talk").secondary(),
            Phase::Listening { locked: true, .. } => text("Typing as you talk, press again to stop").tone(Tone::Red),
            Phase::Listening { .. } => text("Listening, let go to finish").tone(Tone::Red),
            Phase::Writing => text("Writing…").secondary(),
        };
        row![keycaps(caps), state, spacer(), button(self.toggle_label()).on_press(Msg::Toggle)].spacing(8.0).into()
    }

    fn toggle_label(&self) -> &'static str {
        if matches!(self.phase, Phase::Listening { .. }) { "Finish" } else { "Start" }
    }

    fn problem_card(&self) -> Option<Element<Msg>> {
        let problem = self.problem.as_ref()?;
        let mut line =
            row![icon(Icon::Exclamation).size(GLYPH).tone(Tone::Orange), text(&problem.says).wrap()].spacing(8.0);
        if problem.fix.is_some() {
            line = line.push(spacer()).push(button(problem.button).on_press(Msg::Fix));
        }
        Some(card(line))
    }

    fn history_list(&self) -> Element<Msg> {
        if self.history.is_empty() {
            return card(text("What you dictate shows here too, in case it didn't land").secondary());
        }
        list(self.history.iter().enumerate().map(|(index, said)| {
            row![text(said).wrap(), spacer(), button("Copy").plain().on_press(Msg::Copy(index))].spacing(8.0).into()
        }))
    }

    fn glossary_card(&self) -> Element<Msg> {
        let add = row![
            text_input(&self.draft_term)
                .placeholder("Add a word, or heard => meant")
                .on_change(Msg::DraftTerm)
                .on_submit(Msg::AddTerm),
            button("Add").on_press(Msg::AddTerm).enabled(!self.draft_term.trim().is_empty()),
        ]
        .spacing(8.0);
        let mut terms: Vec<(String, Source)> = self.glossary.clone();
        terms.sort_by_key(|(text, source)| (*source != Source::Added, text.to_lowercase()));
        let shown: Vec<Element<Msg>> = terms
            .iter()
            .take(SHOWN_TERMS)
            .map(|(term, source)| {
                let mark = match source {
                    Source::Added => icon(Icon::Star).size(11.0).tone(Tone::Secondary),
                    Source::Corrected | Source::Heard => icon(Icon::Spark).size(11.0).tone(Tone::Yellow),
                };
                row![
                    mark,
                    text(term),
                    spacer(),
                    button("").icon(Icon::Xmark).plain().on_press(Msg::RemoveTerm(term.clone()))
                ]
                .spacing(6.0)
                .into()
            })
            .collect();
        let body: Element<Msg> = if shown.is_empty() {
            text("Names and terms you add, correct or keep saying are learned here").secondary().into()
        } else {
            list(shown)
        };
        let more = terms.len().saturating_sub(SHOWN_TERMS);
        let mut parts = column![text("Your words").section(), add, body].spacing(8.0);
        if more > 0 {
            parts = parts.push(text(format!("and {more} more")).caption().secondary());
        }
        parts.into()
    }
}

impl<B: Backend> Tool for Dictation<B> {
    type Msg = Msg;

    fn manifest() -> Manifest {
        let hotkey = if cfg!(windows) { "ctrl_win" } else { "fn" };
        let manifest = Manifest::new(ID, "Dictation")
            .description("Hold a key and talk: your words are typed where the cursor is")
            .version(env!("CARGO_PKG_VERSION"))
            .page("Dictation")
            .page_icon(Icon::Microphone)
            .shortcut(TOGGLE, "Start or finish dictation", None)
            .setting(
                SettingSpec::text(KEY, "Groq API key", "")
                    .secret()
                    .placeholder("gsk_…")
                    .detail("Free at console.groq.com/keys: sign in, press Create API Key, and paste it here"),
            )
            .setting(
                SettingSpec::choice(HOTKEY, "Dictation key", Hotkey::CHOICES, hotkey)
                    .detail("Hold it to talk, or double-tap it to type as you talk until you press it again or Esc"),
            )
            .setting(
                SettingSpec::choice(MODEL, "Whisper", [("accurate", "Most accurate"), ("fast", "Fastest")], "accurate")
                    .detail("Both take about a second. The other one stands in when Groq is busy"),
            )
            .setting(
                SettingSpec::choice(LANGUAGE, "Language", LANGUAGES, "en")
                    .detail("Picking yours is faster and steadier than detecting it"),
            )
            .setting(
                SettingSpec::toggle(POLISH, "Tidy with AI", true)
                    .detail("Fixes your terms, punctuation and ums with a quick Groq model. Adds about half a second"),
            )
            .setting(SettingSpec::toggle(LEARN, "Learn my words", true).detail(if cfg!(target_os = "macos") {
                "Learns names and terms from what you correct after dictating, and what you say often"
            } else {
                "Learns names and terms you say often"
            }))
            .setting(
                SettingSpec::toggle(SCREEN, "Use words on screen", true)
                    .detail("Reads the window title and the text near the cursor to spell names and terms right"),
            );
        if cfg!(target_os = "macos") {
            manifest.setting(
                SettingSpec::toggle(SOUNDS, "Sounds", true).detail("A soft tick when listening starts and stops"),
            )
        } else {
            manifest.setting(SettingSpec::toggle(SOUNDS, "Sounds", false).hidden())
        }
    }

    fn init(context: &Context) -> (Self, Command<Msg>) {
        let prefs = prefs(context.settings());
        let (engine, pump) = match B::start(context, prefs.clone()) {
            Some((engine, reports)) => (
                Some(engine),
                Command::spawn(move |emitter| {
                    while let Ok(report) = reports.recv() {
                        emitter.send(Msg::Report(Box::new(report)));
                    }
                }),
            ),
            None => (None, Command::none()),
        };
        let dictation = Dictation {
            engine,
            prefs,
            phase: Phase::Idle,
            levels: VecDeque::new(),
            history: VecDeque::new(),
            glossary: Vec::new(),
            problem: None,
            draft_key: String::new(),
            draft_term: String::new(),
            backend: PhantomData,
        };
        (dictation, pump)
    }

    fn update(&mut self, msg: Msg) -> Command<Msg> {
        match msg {
            Msg::Report(report) => self.reported(*report),
            Msg::Configured(settings) => {
                let next = prefs(&settings);
                if next != self.prefs {
                    self.prefs = next.clone();
                    self.control(Control::Prefs(Box::new(next)));
                }
                Command::none()
            }
            Msg::Toggle => {
                self.control(Control::Toggle);
                Command::none()
            }
            Msg::Cancel => {
                self.control(Control::Cancel);
                Command::none()
            }
            Msg::Retry => {
                self.control(Control::Retry);
                Command::dismiss_notice()
            }
            Msg::Setup => Command::dismiss_notice().and(Command::open_page()),
            Msg::Fix => {
                let fix = self.problem.as_ref().and_then(|problem| problem.fix.clone());
                Command::dismiss_notice().and(fix.map_or_else(Command::none, Command::open))
            }
            Msg::GetKey => Command::open(KEYS_PAGE),
            Msg::DraftKey(draft) => {
                self.draft_key = draft;
                Command::none()
            }
            Msg::SaveKey => {
                let key = self.draft_key.trim().to_owned();
                self.draft_key.clear();
                if key.is_empty() {
                    return Command::none();
                }
                self.prefs.key.clone_from(&key);
                self.control(Control::Prefs(Box::new(self.prefs.clone())));
                Command::save_setting(KEY, key)
            }
            Msg::DraftTerm(draft) => {
                self.draft_term = draft;
                Command::none()
            }
            Msg::AddTerm => {
                let entry = std::mem::take(&mut self.draft_term);
                if !entry.trim().is_empty() {
                    self.control(Control::Add(entry));
                }
                Command::none()
            }
            Msg::RemoveTerm(term) => {
                self.glossary.retain(|(text, _)| *text != term);
                self.control(Control::Remove(term));
                Command::none()
            }
            Msg::Copy(index) => match self.history.get(index) {
                Some(text) => Command::copy_text(text.clone()).and(Command::notify("Copied")),
                None => Command::none(),
            },
        }
    }

    fn subscriptions(&self) -> Subscription<Msg> {
        Subscription::batch([Subscription::shortcut(TOGGLE, Msg::Toggle), Subscription::settings(Msg::Configured)])
    }

    fn tile(&self) -> Option<Tile<Msg>> {
        let tile = Tile::new("Dictation").shortcut(TOGGLE).on_press(Msg::Setup);
        Some(match self.phase {
            Phase::Listening { since, .. } => tile
                .title("Listening")
                .symbol(icon(Icon::Microphone).size(15.0).tone(Tone::White))
                .well(Tone::Red)
                .tint(Tone::Red)
                .on_press(Msg::Toggle)
                .footer(timer(since).style(TextStyle::Display)),
            Phase::Writing => tile.title("Writing…").icon(Icon::Microphone),
            Phase::Idle if self.prefs.key.is_empty() => {
                tile.icon(Icon::Microphone).footer(text("Add a Groq key to start").subheadline().secondary())
            }
            Phase::Idle => tile.icon(Icon::Microphone).footer(keycaps(self.prefs.hotkey.caps().to_vec())),
        })
    }

    fn page(&self) -> Option<Element<Msg>> {
        let mut parts: Vec<Element<Msg>> = Vec::new();
        if self.prefs.key.is_empty() {
            parts.push(self.setup_card());
        } else {
            parts.push(self.how_to());
        }
        if let Some(problem) = self.problem_card() {
            parts.push(problem);
        }
        if !self.prefs.key.is_empty() {
            parts.push(text("Recent").section().into());
            parts.push(self.history_list());
        }
        parts.push(self.glossary_card());
        Some(column(parts).spacing(10.0).into())
    }

    fn live(&self) -> Option<Live<Msg>> {
        match self.phase {
            Phase::Idle => None,
            Phase::Listening { locked, .. } => {
                let mic = icon(Icon::Microphone).size(GLYPH).tone(Tone::Red);
                let trailing: Element<Msg> = if locked {
                    row![icon(Icon::Lock).size(10.0).tone(Tone::Secondary), self.bars()].spacing(5.0).into()
                } else {
                    self.bars()
                };
                let controls = row![
                    button("").icon(Icon::Xmark).on_press(Msg::Cancel),
                    button("").icon(Icon::Checkmark).on_press(Msg::Toggle),
                ]
                .spacing(6.0);
                Some(Live::new(mic, trailing).controls(controls))
            }
            Phase::Writing => {
                let mic = icon(Icon::Microphone).size(GLYPH).tone(Tone::Secondary);
                Some(Live::new(mic, text("Writing").secondary()))
            }
        }
    }
}

#[cfg(test)]
mod tests;
