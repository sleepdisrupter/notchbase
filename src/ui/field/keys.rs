//! Turns key events into edits and caret moves: by letter, by word with Option or Ctrl, and Enter or Escape to finish.

use egui::{Event, Key as Pressed, Modifiers};

use super::edit::Line;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Key {
    Typed,
    Submitted,
    Cancelled,
}

pub(super) fn type_into(ui: &egui::Ui, line: &mut Line) -> Vec<Key> {
    let events = ui.input(|input| input.events.clone());
    events.iter().filter_map(|event| apply(line, event)).collect()
}

fn apply(line: &mut Line, event: &Event) -> Option<Key> {
    let changed = |changed: bool| changed.then_some(Key::Typed);
    match event {
        Event::Text(text) | Event::Paste(text) => changed(line.insert(text)),
        Event::Key { key, pressed: true, modifiers, .. } => match key {
            Pressed::Backspace => changed(line.erase_back(by_word(modifiers) || modifiers.mac_cmd)),
            Pressed::Delete => changed(line.erase_forward(by_word(modifiers) || modifiers.mac_cmd)),
            Pressed::Enter => Some(Key::Submitted),
            Pressed::Escape => Some(Key::Cancelled),
            _ => {
                steer(line, *key, modifiers);
                None
            }
        },
        _ => None,
    }
}

fn steer(line: &mut Line, key: Pressed, modifiers: &Modifiers) {
    match key {
        Pressed::ArrowLeft if modifiers.mac_cmd => line.home(),
        Pressed::ArrowRight if modifiers.mac_cmd => line.end(),
        Pressed::ArrowLeft => line.left(by_word(modifiers)),
        Pressed::ArrowRight => line.right(by_word(modifiers)),
        Pressed::Home => line.home(),
        Pressed::End => line.end(),
        Pressed::A if modifiers.command => line.select_all(),
        _ => {}
    }
}

fn by_word(modifiers: &Modifiers) -> bool {
    modifiers.alt || modifiers.command
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(key: Pressed, modifiers: Modifiers) -> Event {
        Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers }
    }

    fn after(line: &mut Line, events: &[Event]) -> Vec<Key> {
        events.iter().filter_map(|event| apply(line, event)).collect()
    }

    #[test]
    fn moving_the_caret_is_not_an_edit() {
        let mut line = Line::new("car", false);
        let keys = after(
            &mut line,
            &[
                press(Pressed::ArrowLeft, Modifiers::NONE),
                Event::Text("e".into()),
                press(Pressed::Home, Modifiers::NONE),
                press(Pressed::Delete, Modifiers::NONE),
            ],
        );
        assert_eq!(keys, [Key::Typed, Key::Typed]);
        assert_eq!(line.text, "aer");
    }

    #[test]
    fn command_a_selects_everything() {
        let mut line = Line::new("cargo", false);
        let keys = after(&mut line, &[press(Pressed::A, Modifiers::COMMAND), Event::Text("x".into())]);
        assert_eq!((keys.as_slice(), line.text.as_str()), ([Key::Typed].as_slice(), "x"));
    }

    #[test]
    fn command_and_option_arrows() {
        let mut line = Line::new("one two", false);
        after(&mut line, &[press(Pressed::ArrowLeft, Modifiers::MAC_CMD)]);
        assert_eq!(line.caret, 0);
        after(&mut line, &[press(Pressed::ArrowRight, Modifiers::ALT)]);
        assert_eq!(line.caret, 3);
        after(&mut line, &[press(Pressed::ArrowRight, Modifiers::MAC_CMD)]);
        assert_eq!(line.caret, 7);
    }

    #[test]
    fn enter_and_escape_are_reported() {
        let mut line = Line::new("a", false);
        let keys = after(
            &mut line,
            &[
                press(Pressed::Tab, Modifiers::NONE),
                press(Pressed::Enter, Modifiers::COMMAND),
                press(Pressed::Escape, Modifiers::NONE),
            ],
        );
        assert_eq!(keys, [Key::Submitted, Key::Cancelled]);
        assert_eq!(line.text, "a");
    }
}
