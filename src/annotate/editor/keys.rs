//! Keyboard shortcuts: digits pick a colour, brackets nudge the size, B and T pick the tool, Command-Z undoes, and
//! Escape and Return finish or close. While typing the plain keys type instead, and a held key never closes the window.

use std::mem;

use egui::{Event, InputState, Key, Modifiers};

use super::command::Command;
use super::layout::SWATCHES;
use super::tool::Tool;

type Chord = (Modifiers, Key);

const NUDGE: f32 = 1.0;
const DIGITS: [Key; SWATCHES] = [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5];
const REDO: Modifiers = Modifiers::COMMAND.plus(Modifiers::SHIFT);
const CLOSE: Chord = (Modifiers::COMMAND, Key::W);
const LEAVE: Chord = (Modifiers::NONE, Key::Escape);
const DONE: Chord = (Modifiers::NONE, Key::Enter);
const STRUCK: [Chord; 3] = [CLOSE, LEAVE, DONE];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Busy {
    Idle,
    Drawing,
    Typing,
}

pub(super) fn commands(ui: &egui::Ui, busy: Busy) -> Vec<Command> {
    let typing = busy == Busy::Typing;
    let (leave, done) = match busy {
        Busy::Idle => (Command::Close, Command::Close),
        Busy::Drawing => (Command::Cancel, Command::Close),
        Busy::Typing => (Command::Finish, Command::Finish),
    };
    let steps = [(REDO, Key::Z, Command::Redo), (Modifiers::COMMAND, Key::Z, Command::Undo)];
    let plain = (!typing).then(plain_keys).into_iter().flatten();
    let struck = [(CLOSE, Command::Close), (LEAVE, leave), (DONE, done)];
    ui.input_mut(|input| {
        let mut commands = Vec::new();
        for (modifiers, key, command) in steps.into_iter().chain(plain) {
            let presses = input.count_and_consume_key(modifiers, key);
            commands.extend(std::iter::repeat_n(command, presses));
        }
        for (chord, command) in struck {
            let strikes = consume_strikes(input, chord, typing);
            commands.extend(std::iter::repeat_n(command, strikes));
        }
        commands
    })
}

/// Drops repeats of Escape, Return and Command-W the window reported, so holding one never goes on to close.
pub(in crate::annotate) fn forget_repeated_strikes(heard: &mut Vec<Event>, already_heard: usize) {
    let repeats = heard.split_off(already_heard.min(heard.len()));
    heard.extend(repeats.into_iter().filter(|event| !STRUCK.iter().any(|&chord| strikes(event, chord))));
}

pub(super) fn typed_before_any_command(ui: &egui::Ui) -> Vec<Event> {
    ui.input_mut(|input| {
        let first_command = input.events.iter().position(commands_while_typing);
        input.events.drain(..first_command.unwrap_or(input.events.len())).collect()
    })
}

pub(super) fn typed_and_not_yet_read(unread: &mut Vec<Event>) -> Vec<Event> {
    let ends = unread.iter().position(|event| commands_while_typing(event) || is_a_press(event));
    let mut later = unread.split_off(ends.unwrap_or(unread.len()));
    if later.first().is_some_and(only_finishes) {
        later.remove(0);
    }
    let (typed, kept) = mem::take(unread).into_iter().partition(comes_from_the_keyboard);
    *unread = kept;
    unread.append(&mut later);
    typed
}

pub(super) fn breaks_the_line(event: &Event) -> bool {
    matches!(event, Event::Key { key: Key::Enter, pressed: true, modifiers, .. } if modifiers.shift)
}

fn commands_while_typing(event: &Event) -> bool {
    let chords = STRUCK.into_iter().chain([(Modifiers::COMMAND, Key::Z)]);
    !breaks_the_line(event) && chords.into_iter().any(|chord| strikes(event, chord))
}

fn only_finishes(event: &Event) -> bool {
    !breaks_the_line(event) && [LEAVE, DONE].into_iter().any(|chord| strikes(event, chord))
}

fn is_a_press(event: &Event) -> bool {
    matches!(event, Event::PointerButton { pressed: true, .. })
}

fn comes_from_the_keyboard(event: &Event) -> bool {
    matches!(event, Event::Text(_) | Event::Paste(_) | Event::Key { .. })
}

fn plain_keys() -> impl Iterator<Item = (Modifiers, Key, Command)> {
    let nudges = [(Key::OpenBracket, Command::Nudge(-NUDGE)), (Key::CloseBracket, Command::Nudge(NUDGE))];
    let digits = DIGITS.into_iter().enumerate().map(|(index, key)| (key, Command::Pick(index)));
    let tools = [(Key::B, Command::Use(Tool::Brush)), (Key::T, Command::Use(Tool::Text))];
    let keys = nudges.into_iter().chain(digits).chain(tools);
    keys.map(|(key, command)| (Modifiers::NONE, key, command))
}

fn consume_strikes(input: &mut InputState, chord: Chord, typing: bool) -> usize {
    let mut fresh = 0;
    input.events.retain(|event| match event {
        Event::Key { repeat, .. } if strikes(event, chord) && !(typing && breaks_the_line(event)) => {
            fresh += usize::from(!repeat);
            false
        }
        _ => true,
    });
    fresh
}

fn strikes(event: &Event, (modifiers, key): Chord) -> bool {
    matches!(event, Event::Key { key: pressed_key, modifiers: with, pressed: true, .. }
        if *pressed_key == key && with.matches_logically(modifiers))
}
