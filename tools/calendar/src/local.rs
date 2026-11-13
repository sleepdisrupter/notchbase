//! Events kept on this computer, for when there's no calendar to save them to.

use std::path::{Path, PathBuf};

use notchbase_sdk::prelude::*;
use serde::{Deserialize, Serialize};

use crate::meeting::Meeting;

pub const CALENDAR_ID: &str = "notchbase";
const FILE: &str = "events.json";
const PREFIX: &str = "local:";
const NAME: &str = "On this computer";
const COLOR: Tone = Tone::Purple;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Saved {
    pub id: String,
    pub title: String,
    pub start: u64,
    pub end: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub link: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Local {
    events: Vec<Saved>,
    file: Option<PathBuf>,
}

impl Local {
    pub fn open(dir: Option<&Path>) -> Local {
        let file = dir.map(|dir| dir.join(FILE));
        let events = file
            .as_deref()
            .and_then(|file| std::fs::read_to_string(file).ok())
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Local { events, file }
    }

    pub fn save(&mut self, draft: &EventDraft, now: u64) -> Command<crate::Msg> {
        let saved = Saved {
            id: draft.id.clone().filter(|id| owns(id)).unwrap_or_else(|| self.fresh_id(now)),
            title: draft.title.trim().to_owned(),
            start: draft.start,
            end: draft.end.max(draft.start),
            link: draft.url.as_deref().unwrap_or_default().trim().to_owned(),
        };
        match self.events.iter_mut().find(|event| event.id == saved.id) {
            Some(event) => *event = saved,
            None => self.events.push(saved),
        }
        self.events.sort_by_key(|event| (event.start, event.end));
        self.persist()
    }

    pub fn delete(&mut self, id: &str) -> Command<crate::Msg> {
        self.events.retain(|event| event.id != id);
        self.persist()
    }

    pub fn meetings(&self, from: u64, to: u64) -> Vec<Meeting> {
        self.events.iter().map(meeting).filter(|meeting| meeting.overlaps(from, to)).collect()
    }

    fn fresh_id(&self, now: u64) -> String {
        (0..)
            .map(|n| format!("{PREFIX}{now}-{n}"))
            .find(|id| !self.events.iter().any(|event| event.id == *id))
            .unwrap_or_default()
    }

    fn persist(&self) -> Command<crate::Msg> {
        let Some(file) = self.file.clone() else { return Command::none() };
        let events = self.events.clone();
        Command::perform(move || write(&file, &events), crate::Msg::Stored)
    }
}

pub fn owns(id: &str) -> bool {
    id.starts_with(PREFIX)
}

pub fn calendar() -> CalendarInfo {
    CalendarInfo {
        id: CALENDAR_ID.into(),
        title: NAME.into(),
        account: String::new(),
        color: Some(COLOR.into()),
        writable: true,
    }
}

fn meeting(saved: &Saved) -> Meeting {
    Meeting {
        id: saved.id.clone(),
        title: crate::meeting::named(&saved.title),
        start: saved.start,
        end: saved.end,
        all_day: false,
        location: String::new(),
        join: crate::join::direct(&saved.link),
        link: saved.link.clone(),
        calendar: NAME.into(),
        color: Some(COLOR.into()),
        declined: false,
        calendar_id: CALENDAR_ID.into(),
        editable: true,
    }
}

fn write(file: &Path, events: &[Saved]) -> Result<(), String> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(|err| err.to_string())?;
    }
    let text = serde_json::to_string_pretty(events).map_err(|err| err.to_string())?;
    let spare = file.with_extension("json.new");
    std::fs::write(&spare, text).map_err(|err| err.to_string())?;
    std::fs::rename(&spare, file).map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(id: Option<&str>, title: &str, start: u64) -> EventDraft {
        EventDraft {
            id: id.map(str::to_owned),
            calendar_id: CALENDAR_ID.into(),
            title: title.into(),
            start,
            end: start + 30,
            ..EventDraft::default()
        }
    }

    #[test]
    fn events_are_added_changed_and_removed() {
        let mut local = Local::default();
        local.save(&draft(None, "Gym", 100), 7);
        local.save(&draft(None, "Call", 50), 7);
        let found = local.meetings(0, 1_000);
        assert_eq!(found.iter().map(|meeting| meeting.title.as_str()).collect::<Vec<_>>(), ["Call", "Gym"]);
        assert!(found.iter().all(|meeting| owns(&meeting.id) && meeting.editable));
        assert_ne!(found[0].id, found[1].id);
        let gym = found[1].id.clone();
        local.save(&draft(Some(&gym), "Gym, legs", 200), 8);
        assert_eq!(local.meetings(150, 300)[0].title, "Gym, legs");
        local.delete(&gym);
        assert_eq!(local.meetings(0, 1_000).len(), 1);
    }

    #[test]
    fn events_survive_a_restart() {
        let dir = std::env::temp_dir().join(format!("notchbase-local-events-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let saved =
            vec![Saved { id: "local:1-0".into(), title: "Dentist".into(), start: 10, end: 20, link: String::new() }];
        write(&dir.join(FILE), &saved).unwrap();
        let local = Local::open(Some(&dir));
        assert_eq!(local.meetings(0, 100)[0].title, "Dentist");
        assert!(Local::open(Some(&dir.join("missing"))).meetings(0, 100).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
