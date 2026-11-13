//! Colors the user picks for events, saved per series so every repeat gets the same one.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use notchbase_sdk::prelude::*;

use crate::meeting::Meeting;

const FILE: &str = "colors.json";
pub const PALETTE: [Tone; 8] =
    [Tone::Red, Tone::Orange, Tone::Yellow, Tone::Green, Tone::Teal, Tone::Blue, Tone::Purple, Tone::Pink];

/// A new event has no id until its calendar saves it, so its color waits here, matched by title
/// and start.
#[derive(Clone, Debug, PartialEq)]
struct Pending {
    title: String,
    start: u64,
    tone: Option<Tone>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Paints {
    chosen: BTreeMap<String, Tone>,
    pending: Vec<Pending>,
    file: Option<PathBuf>,
    dirty: bool,
}

impl Paints {
    pub fn open(dir: Option<&Path>) -> Paints {
        let file = dir.map(|dir| dir.join(FILE));
        let chosen = file
            .as_deref()
            .and_then(|file| std::fs::read_to_string(file).ok())
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Paints { chosen, file, ..Paints::default() }
    }

    pub fn tone(&self, id: &str) -> Option<Tone> {
        self.chosen.get(series(id)).copied()
    }

    pub fn choose(&mut self, id: Option<&str>, title: &str, start: u64, tone: Option<Tone>) {
        if let Some(id) = id {
            self.set(id, tone);
        }
        self.pending.retain(|pending| pending.title != title || pending.start != start);
        self.pending.push(Pending { title: title.to_owned(), start, tone });
    }

    pub fn paint(&mut self, meetings: &mut [Meeting]) {
        for meeting in meetings.iter_mut() {
            let found = self.pending.iter().position(|pending| {
                meeting.editable && pending.start == meeting.start && pending.title == meeting.title
            });
            if let Some(index) = found {
                let pending = self.pending.remove(index);
                self.set(&meeting.id, pending.tone);
            }
            if let Some(tone) = self.tone(&meeting.id) {
                meeting.color = Some(tone.color());
            }
        }
    }

    pub fn flush(&mut self) -> Command<crate::Msg> {
        let file = self.file.clone().filter(|_| std::mem::take(&mut self.dirty));
        let Some(file) = file else { return Command::none() };
        let chosen = self.chosen.clone();
        Command::perform(move || write(&file, &chosen), crate::Msg::Stored)
    }

    fn set(&mut self, id: &str, tone: Option<Tone>) {
        let key = series(id).to_owned();
        let changed = match tone {
            Some(tone) => self.chosen.insert(key, tone) != Some(tone),
            None => self.chosen.remove(&key).is_some(),
        };
        self.dirty |= changed;
    }
}

fn series(id: &str) -> &str {
    match id.rsplit_once('@') {
        Some((series, start)) if !start.is_empty() && start.bytes().all(|byte| byte.is_ascii_digit()) => series,
        _ => id,
    }
}

fn write(file: &Path, chosen: &BTreeMap<String, Tone>) -> Result<(), String> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(|err| err.to_string())?;
    }
    let text = serde_json::to_string_pretty(chosen).map_err(|err| err.to_string())?;
    let spare = file.with_extension("json.new");
    std::fs::write(&spare, text).map_err(|err| err.to_string())?;
    std::fs::rename(&spare, file).map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meeting(id: &str, title: &str, start: u64) -> Meeting {
        Meeting {
            id: id.into(),
            title: title.into(),
            start,
            end: start + 30,
            all_day: false,
            location: String::new(),
            join: None,
            link: String::new(),
            calendar: String::new(),
            color: None,
            declined: false,
            calendar_id: String::new(),
            editable: true,
        }
    }

    #[test]
    fn color_covers_every_occurrence() {
        let mut paints = Paints::default();
        paints.choose(Some("abc@100"), "Standup", 100, Some(Tone::Green));
        let mut meetings = [meeting("abc@100", "Standup", 100), meeting("abc@900", "Standup", 900)];
        paints.paint(&mut meetings);
        assert!(meetings.iter().all(|meeting| meeting.color == Some(Tone::Green.color())));
        assert_eq!(series("x@google.com@17"), "x@google.com");
        assert_eq!(series("local:5-0"), "local:5-0");
    }

    #[test]
    fn new_event_gets_color_with_id() {
        let mut paints = Paints::default();
        paints.choose(None, "Gym", 500, Some(Tone::Orange));
        let mut early = [meeting("other", "Gym", 400)];
        paints.paint(&mut early);
        assert_eq!(early[0].color, None);
        let mut later = [meeting("new@500", "Gym", 500)];
        paints.paint(&mut later);
        assert_eq!(later[0].color, Some(Tone::Orange.color()));
        assert_eq!(paints.tone("new@501"), Some(Tone::Orange));
        assert!(paints.dirty);
        paints.choose(Some("new@500"), "Gym", 500, None);
        assert_eq!(paints.tone("new@500"), None, "the calendar's own color comes back");
    }
}
