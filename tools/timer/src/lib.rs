//! Timer: a countdown set with wheels, shown as a draining ring, with an alarm at the end.

mod clock;
mod countdown;
mod length;
mod page;

use std::marker::PhantomData;
use std::time::Duration;

use notchbase_sdk::prelude::*;

pub use clock::{Clock, SystemClock};
use countdown::Countdown;
use length::Length;

pub const ID: &str = "timer";
const TOGGLE: &str = "toggle";
const LAST: &str = "last_ms";
const RUNNING: &str = "countdown";
const FIRST_MS: u64 = 5 * 60_000;
const TICK: Duration = Duration::from_secs(1);
const LIVE_RING: f32 = 13.0;
const BADGE: f32 = 18.0;

pub struct Timer<C: Clock = SystemClock> {
    length: Length,
    countdown: Countdown,
    clock: PhantomData<C>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    Pick(usize, usize),
    Start,
    Pause,
    Resume,
    Cancel,
    Toggle,
    Tick,
    Open,
    Configured(Settings),
}

impl<C: Clock> Tool for Timer<C> {
    type Msg = Msg;

    fn manifest() -> Manifest {
        Manifest::new(ID, "Timer")
            .description("A countdown with a wheel picker, a draining ring and an alarm")
            .version(env!("CARGO_PKG_VERSION"))
            .page("Timer")
            .shortcut(TOGGLE, "Start or Pause Timer", Some("Ctrl+Alt+T"))
            .setting(SettingSpec::data(LAST, Value::from(FIRST_MS)))
            .setting(SettingSpec::data(RUNNING, Value::from("")))
    }

    fn init(context: &Context) -> (Self, Command<Msg>) {
        let settings = context.settings();
        let countdown = Countdown::restore(settings.text(RUNNING), C::now());
        // A saved countdown that ran out while the app was closed is cleared.
        let forget = if countdown == Countdown::Idle && !settings.text(RUNNING).is_empty() {
            Countdown::Idle.remember()
        } else {
            Command::none()
        };
        (Self { length: remembered(settings), countdown, clock: PhantomData }, forget)
    }

    fn update(&mut self, msg: Msg) -> Command<Msg> {
        let now = C::now();
        match msg {
            Msg::Pick(column, index) => {
                self.length = self.length.set(column, index);
                Command::none()
            }
            Msg::Start => self.start(now),
            Msg::Pause => {
                self.countdown = self.countdown.pause(now);
                self.countdown.remember()
            }
            Msg::Resume => {
                self.countdown = self.countdown.resume(now);
                self.countdown.remember()
            }
            Msg::Cancel => {
                self.countdown = Countdown::Idle;
                self.countdown.remember()
            }
            Msg::Toggle => match self.countdown {
                Countdown::Idle => self.start(now),
                Countdown::Running { .. } => self.update(Msg::Pause),
                Countdown::Paused { .. } => self.update(Msg::Resume),
            },
            Msg::Tick if self.countdown.over(now) => {
                self.countdown = Countdown::Idle;
                Command::chime().and(Command::notice(done(self.length))).and(self.countdown.remember())
            }
            Msg::Tick => Command::none(),
            Msg::Open => Command::open_page(),
            Msg::Configured(settings) => {
                if self.countdown == Countdown::Idle {
                    self.length = remembered(&settings);
                }
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
        // The tile opens the page instead of starting, so a running timer can still be cancelled.
        let tile = Tile::new("Timer").icon(Icon::Timer).shortcut(TOGGLE).on_press(Msg::Open);
        Some(match self.countdown {
            Countdown::Idle => tile.footer(footnote(self.length.words())),
            Countdown::Running { end, .. } => tile
                .symbol(icon(Icon::Timer).tone(Tone::White))
                .well(Tone::Orange)
                .tint(Tone::Orange)
                .footer(timer(end).remaining()),
            Countdown::Paused { left, .. } => tile
                .symbol(icon(Icon::Timer).tone(Tone::Orange))
                .footer(footnote(format!("Paused · {}", clock(Duration::from_millis(left.div_ceil(1000) * 1000))))),
        })
    }

    fn page(&self) -> Option<Element<Msg>> {
        Some(page::page(self.length, self.countdown, C::now()))
    }

    fn live(&self) -> Option<Live<Msg>> {
        let now = C::now();
        match self.countdown {
            Countdown::Idle => None,
            Countdown::Running { start, end } => Some(
                Live::new(
                    ring(1.0).size(LIVE_RING).tone(Tone::Orange).counting_down(start, end),
                    timer(end).remaining(),
                )
                .controls(controls(Icon::Pause, Msg::Pause)),
            ),
            Countdown::Paused { left, .. } => Some(
                Live::new(
                    ring(self.countdown.share_left(now)).size(LIVE_RING).tone(Tone::Gray),
                    text(clock(Duration::from_millis(left.div_ceil(1000) * 1000))).value().secondary(),
                )
                .controls(controls(Icon::Play, Msg::Resume)),
            ),
        }
    }
}

impl<C: Clock> Timer<C> {
    fn start(&mut self, now: u64) -> Command<Msg> {
        let length = self.length.ms();
        if length == 0 {
            return Command::none();
        }
        self.countdown = Countdown::start(length, now);
        Command::save_setting(LAST, length).and(self.countdown.remember())
    }
}

fn remembered(settings: &Settings) -> Length {
    let last = settings.number(LAST).round() as u64;
    Length::from_ms(if last == 0 { FIRST_MS } else { last })
}

fn footnote(words: String) -> Element<Msg> {
    text(words).subheadline().secondary().into()
}

fn done(length: Length) -> Notice {
    let words: Element<()> =
        notchbase_sdk::column![text("Timer").callout(), text(length.words()).caption().secondary()].spacing(1.0).into();
    Notice::new(badge(Icon::Timer, Tone::Orange, BADGE), words).shake().sticky() // stays until it is seen
}

fn controls(toggle: Icon, msg: Msg) -> Element<Msg> {
    row![button("").icon(toggle).on_press(msg), button("").icon(Icon::Xmark).on_press(Msg::Cancel)].spacing(6.0).into()
}

impl Countdown {
    fn remember(self) -> Command<Msg> {
        Command::save_setting(RUNNING, self.save())
    }
}

notchbase_sdk::export!(Timer);

#[cfg(test)]
mod tests;
