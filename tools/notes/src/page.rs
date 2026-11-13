//! The Notes page: a box to write in, the saved notes, and a line on what the notch is doing.

use notchbase_sdk::column;
use notchbase_sdk::prelude::*;

use crate::{Msg, Notes};

const ROW_CHARS: usize = 34;

pub fn page(notes: &Notes) -> Element<Msg> {
    let composer = row![
        text_input(&notes.draft).placeholder("Write a note").on_change(Msg::Draft).on_submit(Msg::Add),
        button("Add").filled().enabled(!notes.draft.trim().is_empty()).on_press(Msg::Add),
    ]
    .spacing(8.0);
    let saved = if notes.notes.is_empty() {
        card(text("Nothing yet. Notes you add stay here, and one can live in the notch.").secondary().wrap())
    } else {
        list(notes.notes.iter().enumerate().map(|(index, note)| note_row(index, note, notes.pinned == Some(index))))
    };
    let keep_open = row![
        text("Keep the notch open").caption().secondary(),
        spacer(),
        toggle(notes.holding).on_toggle(Msg::KeepOpen)
    ];
    let mut page = column![composer, saved, keep_open].spacing(10.0);
    if notes.prefs.details {
        page = page.push(text(describe(&notes.notch)).caption().tertiary());
    }
    page.into()
}

fn note_row(index: usize, note: &str, pinned: bool) -> Element<Msg> {
    let pin = chip("Pin", pinned).icon(Icon::Star).on_toggle(move |_| Msg::Pin(index));
    let actions = row![
        pin,
        button("").icon(Icon::Note).plain().on_press(Msg::Copy(index)),
        button("").icon(Icon::Xmark).plain().on_press(Msg::Remove(index)),
    ]
    .spacing(4.0)
    .align(Align::Center);
    keyed(note, row![text(preview(note, ROW_CHARS)), spacer(), actions].spacing(6.0).align(Align::Center))
}

/// The first line of a note, cut to `most` characters. An ellipsis shows when anything was left out.
pub fn preview(note: &str, most: usize) -> String {
    let line = note.lines().next().unwrap_or_default();
    let mut chars = line.chars();
    let short: String = chars.by_ref().take(most).collect();
    if chars.next().is_some() || note.lines().nth(1).is_some() { format!("{short}\u{2026}") } else { short }
}

pub fn describe(state: &NotchState) -> String {
    let phase = match state.phase {
        NotchPhase::Resting => "Resting",
        NotchPhase::Peeking => "Peeking",
        NotchPhase::Open => "Open",
        NotchPhase::Notice => "Showing a notice",
        NotchPhase::Dropping => "Catching files",
        NotchPhase::Asking => "Asking",
    };
    let style = match state.style {
        NotchStyle::Notch => "notch",
        NotchStyle::Island => "island",
    };
    let percent = |value: f32| (value * 100.0).round() as u32;
    format!(
        "{phase} {style} \u{b7} {}\u{d7}{} pt \u{b7} open {}% \u{b7} hugging {}%",
        state.width.round(),
        state.height.round(),
        percent(state.expansion),
        percent(state.hug),
    )
}
