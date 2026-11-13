//! The Timer page: wheels while idle, a dial while counting down, and round buttons either side.

use notchbase_sdk::element::Ring;
use notchbase_sdk::prelude::*;
use notchbase_sdk::{column, row};

use crate::Msg;
use crate::countdown::Countdown;
use crate::length::{Length, most_hours};

pub const BUTTON: f32 = 56.0;
const DIAL: f32 = 128.0;
const DIAL_STROKE: f32 = 6.0;
const BELL: f32 = 10.0;
const RUNNING: Tone = Tone::Orange;
const GO: Tone = Tone::Green;

pub fn page(length: Length, countdown: Countdown, now: u64) -> Element<Msg> {
    let (middle, right) = match countdown {
        Countdown::Idle => (wheels(length), start(length)),
        Countdown::Running { start, end } => (dial(countdown, now).counting_down(start, end).into(), pause()),
        Countdown::Paused { .. } => (dial(countdown, now).into(), resume()),
    };
    let cancel = round_button("Cancel").size(BUTTON).enabled(countdown != Countdown::Idle).on_press(Msg::Cancel);
    row![cancel, spacer(), middle, spacer(), right].align(Align::Center).into()
}

fn wheels(length: Length) -> Element<Msg> {
    let numbers = |count: usize| (0..count).map(|n| n.to_string());
    picker([
        wheel(numbers(most_hours() + 1), length.hours as usize).unit("hours").looping(),
        wheel(numbers(60), length.minutes as usize).unit("min").looping(),
        wheel(numbers(60), length.seconds as usize).unit("sec").looping(),
    ])
    .on_change(Msg::Pick)
    .into()
}

fn dial(countdown: Countdown, now: u64) -> Ring {
    let tone = if countdown.running() { RUNNING } else { Tone::Gray };
    ring(countdown.share_left(now)).size(DIAL).stroke(DIAL_STROKE).tone(tone).center(face(countdown, now))
}

fn face(countdown: Countdown, now: u64) -> Element<()> {
    let under: Element<()> = match countdown {
        Countdown::Running { end, .. } => row![
            icon(Icon::Bell).size(BELL).tone(Tone::Secondary),
            timer(end).clock().style(TextStyle::Caption).tone(Tone::Secondary)
        ]
        .spacing(4.0)
        .align(Align::Center)
        .into(),
        _ => text("Paused").caption().secondary().into(),
    };
    let big: Element<()> = match countdown {
        Countdown::Running { end, .. } => timer(end).remaining().style(TextStyle::Hero).into(),
        _ => text(clock(std::time::Duration::from_millis(countdown.left(now).div_ceil(1000) * 1000))).hero().into(),
    };
    column![big, under].spacing(2.0).align(Align::Center).into()
}

fn start(length: Length) -> Element<Msg> {
    round_button("Start").size(BUTTON).tone(GO).enabled(length.ms() > 0).on_press(Msg::Start).into()
}

fn pause() -> Element<Msg> {
    round_button("Pause").size(BUTTON).tone(RUNNING).on_press(Msg::Pause).into()
}

fn resume() -> Element<Msg> {
    round_button("Resume").size(BUTTON).tone(GO).on_press(Msg::Resume).into()
}
