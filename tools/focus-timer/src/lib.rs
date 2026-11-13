//! Focus timer: Pomodoro-style focus sessions with breaks in between, and a count of the sessions done.

mod clock;
mod countdown;
mod page;
mod prefs;

use std::marker::PhantomData;
use std::time::Duration;

use notchbase_sdk::prelude::*;

pub use clock::{Clock, SystemClock};
use countdown::{Countdown, Phase, Run};
use prefs::Prefs;

pub const ID: &str = "focus-timer";
const TOGGLE: &str = "toggle";
const PRESETS: [u64; 3] = [15, 25, 50];
const TICK: Duration = Duration::from_secs(1);
const RING: f32 = 13.0;
const BADGE: f32 = 18.0;
const SHOWN_FOR: Duration = Duration::from_millis(2600);

pub struct FocusTimer<C: Clock = SystemClock> {
    prefs: Prefs,
    sessions: u64,
    countdown: Countdown,
    clock: PhantomData<C>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    Toggle,
    Reset,
    Tick,
    Preset(usize),
    AutoBreak(bool),
    Configured(Settings),
}

impl<C: Clock> Tool for FocusTimer<C> {
    type Msg = Msg;

    fn manifest() -> Manifest {
        Prefs::declare(
            Manifest::new(ID, "Focus")
                .description("A Pomodoro-style focus timer with breaks")
                .version(env!("CARGO_PKG_VERSION"))
                .page("Focus")
                .page_icon(Icon::Clock)
                .shortcut(TOGGLE, "Start or Pause Focus", Some("Ctrl+Alt+F")),
        )
    }

    fn init(context: &Context) -> (Self, Command<Msg>) {
        let prefs = Prefs::from(context.settings());
        let countdown = Countdown::idle(prefs.focus_ms());
        let sessions = prefs::sessions(context.settings());
        (Self { prefs, sessions, countdown, clock: PhantomData }, Command::none())
    }

    fn update(&mut self, msg: Msg) -> Command<Msg> {
        let now = C::now();
        match msg {
            Msg::Toggle => {
                self.countdown.toggle(now);
                Command::none()
            }
            Msg::Reset => {
                self.countdown = self.ready();
                Command::none()
            }
            Msg::Tick if self.countdown.over(now) => self.finish(now),
            Msg::Tick => Command::none(),
            Msg::Preset(index) => self.preset(index),
            Msg::AutoBreak(on) => {
                self.prefs.auto_break = on;
                Command::save_setting(prefs::AUTO_BREAK, on)
            }
            Msg::Configured(settings) => {
                self.prefs = Prefs::from(&settings);
                self.sessions = prefs::sessions(&settings);
                self.refresh();
                Command::none()
            }
        }
    }

    fn subscriptions(&self) -> Subscription<Msg> {
        let listening = [Subscription::shortcut(TOGGLE, Msg::Toggle), Subscription::settings(Msg::Configured)];
        let ticking = self.countdown.running().then(|| Subscription::every(TICK, Msg::Tick));
        Subscription::batch(listening.into_iter().chain(ticking))
    }

    fn tile(&self) -> Option<Tile<Msg>> {
        let tile = Tile::new("Focus").icon(Icon::Clock).shortcut(TOGGLE).on_press(Msg::Toggle);
        let tone = self.countdown.phase().tone();
        Some(match (self.countdown.phase(), self.countdown.run()) {
            (Phase::Idle, _) => tile.footer(footnote(page::minutes(self.prefs.focus_minutes))),
            (_, Run::Running { end }) => {
                tile.symbol(icon(Icon::Clock).tone(Tone::White)).well(tone).tint(tone).footer(timer(end).remaining())
            }
            (_, Run::Paused { left }) => {
                let footer = footnote(format!("Paused · {}", page::countdown_clock(left)));
                tile.symbol(icon(Icon::Clock).tone(tone)).footer(footer)
            }
        })
    }

    fn page(&self) -> Option<Element<Msg>> {
        Some(page::page(&self.countdown, &self.prefs, self.sessions, C::now()))
    }

    fn live(&self) -> Option<Live<Msg>> {
        let end = self.countdown.end()?;
        let tone = self.countdown.phase().tone();
        let controls = row![
            button("").icon(Icon::Pause).on_press(Msg::Toggle),
            button("").icon(Icon::Xmark).on_press(Msg::Reset),
        ]
        .spacing(6.0);
        let live = Live::new(ring(self.countdown.progress(C::now())).size(RING).tone(tone), timer(end).remaining());
        Some(live.controls(controls))
    }
}

impl<C: Clock> FocusTimer<C> {
    fn ready(&self) -> Countdown {
        Countdown::idle(self.prefs.focus_ms())
    }

    fn refresh(&mut self) {
        if self.countdown.phase() == Phase::Idle {
            self.countdown = self.ready();
        }
    }

    fn finish(&mut self, now: u64) -> Command<Msg> {
        match self.countdown.phase() {
            Phase::Focus => {
                self.sessions += 1;
                self.countdown = if self.prefs.auto_break {
                    Countdown::start(Phase::Break, self.prefs.break_ms(), now)
                } else {
                    self.ready()
                };
                Command::save_setting(prefs::SESSIONS, self.sessions).and(Command::notice(focus_done()))
            }
            Phase::Break => {
                self.countdown = self.ready();
                Command::notice(break_over())
            }
            Phase::Idle => Command::none(),
        }
    }

    fn preset(&mut self, index: usize) -> Command<Msg> {
        let Some(&minutes) = PRESETS.get(index) else { return Command::none() };
        self.prefs.focus_minutes = minutes;
        self.refresh();
        Command::save_setting(prefs::FOCUS_MINUTES, minutes)
    }
}

fn footnote(words: String) -> Element<Msg> {
    text(words).subheadline().secondary().into()
}

fn focus_done() -> Notice {
    Notice::new(badge(Icon::Checkmark, Tone::Green, BADGE), text("Focus done").callout()).pop().duration(SHOWN_FOR)
}

fn break_over() -> Notice {
    Notice::new(badge(Icon::Bell, Tone::Orange, BADGE), text("Break over").callout()).pop().duration(SHOWN_FOR)
}

notchbase_sdk::export!(FocusTimer);

#[cfg(test)]
mod tests;
