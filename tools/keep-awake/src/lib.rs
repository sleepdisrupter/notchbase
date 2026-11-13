//! Keep awake: stops the computer and its display from sleeping, for a while or until turned off.

mod clock;

use std::marker::PhantomData;
use std::time::Duration;

use notchbase_sdk::prelude::*;

pub use clock::{Clock, SystemClock};

pub const ID: &str = "keep-awake";
const TOGGLE: &str = "toggle";
const LENGTH: &str = "length";
const IN_NOTCH: &str = "in_notch";
const SESSION: &str = "session";
const FOREVER: &str = "forever";
// The host fills these two in from the awake section of config.json. When `forced` is on, the
// computer stays awake whatever the tool does.
pub const FORCED: &str = "forced";
pub const LID: &str = "hold_lid_closed";
const LENGTHS: [(&str, &str); 6] = [
    ("0", "Until I turn it off"),
    ("15", "15 minutes"),
    ("30", "30 minutes"),
    ("60", "1 hour"),
    ("120", "2 hours"),
    ("240", "4 hours"),
];
const MINUTE_MS: u64 = 60_000;
const TICK: Duration = Duration::from_secs(1);
const GLYPH: f32 = 14.0;
const ENDED_FOR: Duration = Duration::from_millis(2400);
const AWAKE: &str = "Awake";
const ALWAYS: &str = "Always awake";
const ENDED: &str = "Sleep allowed";
const LID_TITLE: &str = "Keep awake when the lid is closed";
const LID_DETAIL: &str = "Asks for your password once, the first time";

pub struct KeepAwake<C: Clock = SystemClock> {
    awake: Option<Session>,
    forced: bool,
    prefs: Prefs,
    clock: PhantomData<C>,
}

/// One press of the tile. `until` is None when it lasts until turned off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Session {
    until: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Prefs {
    minutes: u64,
    in_notch: bool,
}

impl Session {
    fn remember(session: Option<Self>) -> Command<Msg> {
        let value = match session {
            None => String::new(),
            Some(Session { until: None }) => FOREVER.to_owned(),
            Some(Session { until: Some(until) }) => until.to_string(),
        };
        Command::save_setting(SESSION, value)
    }

    fn recall(settings: &Settings, now: u64) -> Option<Self> {
        match settings.text(SESSION) {
            FOREVER => Some(Session { until: None }),
            saved => saved.parse().ok().filter(|until| *until > now).map(|until| Session { until: Some(until) }),
        }
    }
}

impl Prefs {
    fn from(settings: &Settings) -> Self {
        Self { minutes: settings.text(LENGTH).parse().unwrap_or(0), in_notch: settings.bool(IN_NOTCH) }
    }

    fn words(self) -> &'static str {
        let chosen = LENGTHS.iter().find(|(minutes, _)| minutes.parse() == Ok(self.minutes));
        chosen.map_or(LENGTHS[0].1, |(_, words)| *words)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    Toggle,
    Stop,
    Tick,
    Configured(Settings),
}

impl<C: Clock> Tool for KeepAwake<C> {
    type Msg = Msg;

    fn manifest() -> Manifest {
        let lid = SettingSpec::toggle(LID, LID_TITLE, true).detail(LID_DETAIL);
        let lid = if cfg!(target_os = "macos") { lid } else { lid.hidden() };
        Manifest::new(ID, "Keep Awake")
            .description("Keep the computer and its display from sleeping")
            .version(env!("CARGO_PKG_VERSION"))
            .shortcut(TOGGLE, "Toggle Keep Awake", None)
            .setting(SettingSpec::choice(LENGTH, "Stay awake for", LENGTHS, "0").detail("How long one press lasts"))
            .setting(SettingSpec::toggle(IN_NOTCH, "Show in the notch", true).detail("A cup while it is on"))
            .setting(lid)
            .setting(SettingSpec::data(SESSION, Value::from(""))) // the running session, so it survives a restart
    }

    fn init(context: &Context) -> (Self, Command<Msg>) {
        let settings = context.settings();
        let forced = settings.bool(FORCED);
        let awake = Session::recall(settings, C::now());
        let restore = match (awake, settings.text(SESSION)) {
            (Some(_), _) => Command::keep_awake(true),
            (None, "") => Command::none(),
            // A saved session that ended while the app was closed is cleared.
            (None, _) => Session::remember(None),
        };
        (Self { awake, forced, prefs: Prefs::from(settings), clock: PhantomData }, restore)
    }

    fn update(&mut self, msg: Msg) -> Command<Msg> {
        match msg {
            Msg::Toggle | Msg::Stop if self.forced => Command::none(),
            Msg::Toggle if self.awake.is_none() => {
                let length = self.prefs.minutes.saturating_mul(MINUTE_MS);
                self.awake = Some(Session { until: (length > 0).then(|| C::now().saturating_add(length)) });
                Command::keep_awake(true).and(Session::remember(self.awake))
            }
            Msg::Toggle | Msg::Stop => self.stop(),
            Msg::Tick if self.over() => self.stop().and(Command::notice(ended())),
            Msg::Tick => Command::none(),
            Msg::Configured(settings) => {
                self.prefs = Prefs::from(&settings);
                self.forced = settings.bool(FORCED);
                Command::none()
            }
        }
    }

    fn subscriptions(&self) -> Subscription<Msg> {
        let listening = [Subscription::shortcut(TOGGLE, Msg::Toggle), Subscription::settings(Msg::Configured)];
        let timed = !self.forced && self.awake.is_some_and(|session| session.until.is_some());
        Subscription::batch(listening.into_iter().chain(timed.then(|| Subscription::every(TICK, Msg::Tick))))
    }

    fn tile(&self) -> Option<Tile<Msg>> {
        let tile = Tile::new("Keep Awake").icon(Icon::Cup).shortcut(TOGGLE).on_press(Msg::Toggle);
        Some(match self.showing() {
            None => tile.footer(text(self.prefs.words()).subheadline().secondary()),
            Some(session) => {
                let tile = tile.symbol(icon(Icon::Cup).tone(Tone::White)).well(Tone::Orange).tint(Tone::Orange);
                match session.until {
                    Some(until) => tile.footer(timer(until).remaining()),
                    None => tile.footer(text(self.words()).subheadline().secondary()),
                }
            }
        })
    }

    fn live(&self) -> Option<Live<Msg>> {
        let session = self.showing().filter(|_| self.prefs.in_notch)?;
        let trailing: Element<Msg> = match session.until {
            Some(until) => timer(until).remaining().tone(Tone::Secondary).into(),
            None => text(self.words()).callout().secondary().into(),
        };
        let live = Live::new(icon(Icon::Cup).size(GLYPH).tone(Tone::Orange), trailing);
        Some(if self.forced { live } else { live.controls(button("").icon(Icon::Xmark).on_press(Msg::Stop)) })
    }
}

impl<C: Clock> KeepAwake<C> {
    fn showing(&self) -> Option<Session> {
        if self.forced { Some(Session { until: None }) } else { self.awake }
    }

    fn words(&self) -> &'static str {
        if self.forced { ALWAYS } else { AWAKE }
    }

    fn over(&self) -> bool {
        self.awake.and_then(|session| session.until).is_some_and(|until| C::now() >= until)
    }

    fn stop(&mut self) -> Command<Msg> {
        match self.awake.take() {
            Some(_) => Command::keep_awake(false).and(Session::remember(None)),
            None => Command::none(),
        }
    }
}

fn ended() -> Notice {
    Notice::new(icon(Icon::Cup).size(GLYPH).tone(Tone::Secondary), text(ENDED).callout().secondary())
        .size(NoticeSize::Small)
        .duration(ENDED_FOR)
}

notchbase_sdk::export!(KeepAwake);

#[cfg(test)]
mod tests;
