//! The system calendar feed, which is Apple Calendar on a Mac.

use notchbase_sdk::prelude::*;

use crate::meeting::Meeting;

#[derive(Clone, Debug, Default, PartialEq)]
pub enum System {
    #[default]
    Unknown,
    Unavailable,
    Waiting,
    Denied,
    Ready {
        events: Vec<Meeting>,
        calendars: Vec<CalendarInfo>,
    },
}

impl System {
    pub fn read(feed: CalendarFeed) -> System {
        match feed {
            CalendarFeed::Unavailable => System::Unavailable,
            CalendarFeed::Waiting => System::Waiting,
            CalendarFeed::Denied => System::Denied,
            CalendarFeed::Events { events, calendars } => System::Ready {
                events: events.iter().map(Meeting::from_event).collect(),
                calendars: calendars.into_iter().filter(|calendar| calendar.writable).collect(),
            },
        }
    }

    pub fn meetings(&self, from: u64, to: u64) -> Vec<Meeting> {
        match self {
            System::Ready { events, .. } => {
                events.iter().filter(|meeting| meeting.overlaps(from, to)).cloned().collect()
            }
            _ => Vec::new(),
        }
    }

    pub fn writable(&self) -> &[CalendarInfo] {
        match self {
            System::Ready { calendars, .. } => calendars,
            _ => &[],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_writable_calendars_offered() {
        let calendar = |id: &str, writable: bool| CalendarInfo { id: id.into(), writable, ..CalendarInfo::default() };
        let system = System::read(CalendarFeed::Events {
            events: Vec::new(),
            calendars: vec![calendar("home", true), calendar("holidays", false)],
        });
        let ids: Vec<&str> = system.writable().iter().map(|calendar| calendar.id.as_str()).collect();
        assert_eq!(ids, ["home"]);
        assert!(System::read(CalendarFeed::Denied).writable().is_empty());
    }
}
