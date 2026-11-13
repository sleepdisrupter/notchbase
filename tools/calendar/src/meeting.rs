//! One event as the tool shows it, whichever calendar it came from.

use notchbase_sdk::protocol::{CalendarEvent, Color};

use crate::join;

#[derive(Clone, Debug, PartialEq)]
pub struct Meeting {
    pub id: String,
    pub title: String,
    pub start: u64,
    pub end: u64,
    pub all_day: bool,
    pub location: String,
    pub join: Option<String>,
    pub link: String,
    pub calendar: String,
    pub color: Option<Color>,
    pub declined: bool,
    pub calendar_id: String,
    pub editable: bool,
}

impl Meeting {
    pub fn from_event(event: &CalendarEvent) -> Meeting {
        Meeting {
            id: event.id.clone(),
            title: named(&event.title),
            start: event.start,
            end: event.end.max(event.start),
            all_day: event.all_day,
            location: event.location.trim().to_owned(),
            join: join::find(&[&event.url, &event.location, &event.notes]).or_else(|| join::direct(&event.url)),
            link: event.url.trim().to_owned(),
            calendar: event.calendar.clone(),
            color: event.color,
            declined: event.declined,
            calendar_id: event.calendar_id.clone(),
            editable: event.editable && !event.all_day,
        }
    }

    pub fn timed(&self) -> bool {
        !self.all_day && !self.declined
    }

    pub fn overlaps(&self, from: u64, to: u64) -> bool {
        self.start < to && self.end.max(self.start + 1) > from
    }

    pub fn same_as(&self, other: &Meeting) -> bool {
        self.start == other.start && self.all_day == other.all_day && self.title.eq_ignore_ascii_case(&other.title)
    }
}

pub fn named(title: &str) -> String {
    match title.trim() {
        "" => "Untitled".to_owned(),
        title => title.to_owned(),
    }
}

pub fn merge(groups: impl IntoIterator<Item = Vec<Meeting>>) -> Vec<Meeting> {
    let mut all: Vec<Meeting> = Vec::new();
    for meeting in groups.into_iter().flatten() {
        if !all.iter().any(|kept| kept.same_as(&meeting)) {
            all.push(meeting);
        }
    }
    all.sort_by(|a, b| (!a.all_day, a.start, a.end, &a.title).cmp(&(!b.all_day, b.start, b.end, &b.title)));
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(title: &str, start: u64) -> Meeting {
        Meeting {
            id: format!("{title}{start}"),
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
            editable: false,
        }
    }

    #[test]
    fn duplicate_meetings_show_once() {
        let merged = merge([vec![at("Standup", 200), at("Lunch", 100)], vec![at("standup", 200), at("Review", 300)]]);
        let titles: Vec<&str> = merged.iter().map(|meeting| meeting.title.as_str()).collect();
        assert_eq!(titles, ["Lunch", "Standup", "Review"]);
    }

    #[test]
    fn apple_event_finds_its_link() {
        let event = CalendarEvent {
            id: "x".into(),
            title: "  ".into(),
            start: 10,
            end: 5,
            notes: "Dial in: https://acme.zoom.us/j/123456?pwd=abc".into(),
            ..CalendarEvent::default()
        };
        let meeting = Meeting::from_event(&event);
        assert_eq!(meeting.title, "Untitled");
        assert_eq!(meeting.end, 10);
        assert_eq!(meeting.join.as_deref(), Some("https://acme.zoom.us/j/123456?pwd=abc"));
    }
}
