//! The Focus page: the countdown, the presets, the controls and the session count.

use std::time::Duration;

use notchbase_sdk::column;
use notchbase_sdk::prelude::*;

use crate::countdown::{Countdown, Phase, Run};
use crate::prefs::Prefs;
use crate::{Msg, PRESETS, TOGGLE};

const SPACING: f32 = 12.0;
const BUTTON_GAP: f32 = 8.0;
const AUTO_BREAK: &str = "Start breaks automatically";

pub fn page(countdown: &Countdown, prefs: &Prefs, sessions: u64, now: u64) -> Element<Msg> {
    let tone = countdown.phase().tone();
    column![
        text(heading(countdown)).section().tone(tone),
        remaining(countdown, now),
        progress(countdown.progress(now)).tone(tone),
        presets(prefs.focus_minutes),
        controls(countdown),
        checkbox(AUTO_BREAK, prefs.auto_break).on_toggle(Msg::AutoBreak),
        text(tally(sessions)).caption().secondary(),
    ]
    .spacing(SPACING)
    .into()
}

pub fn minutes(count: u64) -> String {
    format!("{count} min")
}

/// Time left as a clock, rounded up to the second like the host's own timer.
pub fn countdown_clock(left_ms: u64) -> String {
    clock(Duration::from_millis(left_ms.div_ceil(1000) * 1000))
}

fn heading(countdown: &Countdown) -> String {
    let title = countdown.phase().title();
    match (countdown.phase(), countdown.run()) {
        (Phase::Focus | Phase::Break, Run::Paused { .. }) => format!("{title} · Paused"),
        _ => title.to_owned(),
    }
}

fn remaining(countdown: &Countdown, now: u64) -> Element<Msg> {
    match countdown.end() {
        Some(end) => timer(end).remaining().style(TextStyle::Display).into(),
        None => text(countdown_clock(countdown.left(now))).display().into(),
    }
}

fn presets(focus_minutes: u64) -> Element<Msg> {
    let mut labels: Vec<String> = PRESETS.iter().map(|&preset| minutes(preset)).collect();
    let selected = PRESETS.iter().position(|&preset| preset == focus_minutes).unwrap_or_else(|| {
        labels.push(minutes(focus_minutes));
        PRESETS.len()
    });
    segmented(labels, selected).on_select(Msg::Preset).into()
}

fn controls(countdown: &Countdown) -> Element<Msg> {
    let (label, glyph) = match (countdown.phase(), countdown.run()) {
        (Phase::Idle, _) => ("Start", Icon::Play),
        (_, Run::Running { .. }) => ("Pause", Icon::Pause),
        (_, Run::Paused { .. }) => ("Resume", Icon::Play),
    };
    let primary = button(label).icon(glyph).filled().on_press(Msg::Toggle);
    let reset = button("Reset").icon(Icon::Stop);
    let reset = if countdown.phase() == Phase::Idle { reset } else { reset.on_press(Msg::Reset) };
    row![primary, reset, spacer(), shortcut(TOGGLE)].spacing(BUTTON_GAP).into()
}

fn tally(sessions: u64) -> String {
    match sessions {
        0 => "No focus sessions yet".to_owned(),
        1 => "1 focus session done".to_owned(),
        count => format!("{count} focus sessions done"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_rounds_up_like_the_host() {
        assert_eq!(countdown_clock(25 * 60_000), "25:00");
        assert_eq!(countdown_clock(25 * 60_000 - 400), "25:00");
        assert_eq!(countdown_clock(1), "0:01");
        assert_eq!(countdown_clock(0), "0:00");
        assert_eq!(countdown_clock(90 * 60_000), "1:30:00");
    }

    #[test]
    fn tally_reads_naturally() {
        assert_eq!(tally(0), "No focus sessions yet");
        assert_eq!(tally(1), "1 focus session done");
        assert_eq!(tally(12), "12 focus sessions done");
    }

    #[test]
    fn heading_says_paused() {
        let mut countdown = Countdown::start(Phase::Break, 60_000, 0);
        assert_eq!(heading(&countdown), "Break");
        countdown.toggle(1);
        assert_eq!(heading(&countdown), "Break · Paused");
        assert_eq!(heading(&Countdown::idle(60_000)), "Ready");
    }
}
