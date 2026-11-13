//! Which meeting the notch shows. One about to start beats one already running.

use std::collections::BTreeSet;

use crate::meeting::Meeting;

const LONGEST_MS: u64 = 4 * 60 * 60 * 1000; // a block longer than four hours isn't a meeting
const JUST_STARTED_MS: u64 = 60_000;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase<'a> {
    Soon(&'a Meeting),
    Now(&'a Meeting),
}

impl<'a> Phase<'a> {
    pub fn meeting(self) -> &'a Meeting {
        match self {
            Phase::Soon(meeting) | Phase::Now(meeting) => meeting,
        }
    }
}

pub fn phase<'a>(meetings: &'a [Meeting], now: u64, lead_ms: u64, hidden: &BTreeSet<String>) -> Option<Phase<'a>> {
    let shown = || meetings.iter().filter(|meeting| meeting.timed() && !hidden.contains(&meeting.id));
    let soon = shown()
        .filter(|meeting| meeting.start > now && meeting.start - now <= lead_ms)
        .min_by_key(|meeting| meeting.start);
    if let Some(meeting) = soon {
        return Some(Phase::Soon(meeting));
    }
    shown()
        .filter(|meeting| meeting.start <= now && now < meeting.end && meeting.end - meeting.start <= LONGEST_MS)
        .max_by_key(|meeting| meeting.start)
        .map(Phase::Now)
}

pub fn starting<'a>(meetings: &'a [Meeting], now: u64, nudged: &BTreeSet<String>) -> Vec<&'a Meeting> {
    meetings
        .iter()
        .filter(|meeting| meeting.timed() && !nudged.contains(&meeting.id))
        .filter(|meeting| {
            meeting.start <= now && now < meeting.start + JUST_STARTED_MS && now < meeting.end.max(meeting.start + 1)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: u64 = 60_000;

    fn meeting(id: &str, start: u64, minutes: u64) -> Meeting {
        Meeting {
            id: id.into(),
            title: id.into(),
            start,
            end: start + minutes * MIN,
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
    fn next_meeting_beats_running_one() {
        let meetings = [meeting("design", 0, 60), meeting("standup", 60 * MIN, 15)];
        let hidden = BTreeSet::new();
        assert_eq!(phase(&meetings, 30 * MIN, 10 * MIN, &hidden), Some(Phase::Now(&meetings[0])));
        assert_eq!(phase(&meetings, 52 * MIN, 10 * MIN, &hidden), Some(Phase::Soon(&meetings[1])));
        assert_eq!(phase(&meetings, 62 * MIN, 10 * MIN, &hidden), Some(Phase::Now(&meetings[1])));
        assert_eq!(phase(&meetings, 80 * MIN, 10 * MIN, &hidden), None);
    }

    #[test]
    fn skips_long_declined_and_hidden() {
        let mut declined = meeting("declined", 0, 30);
        declined.declined = true;
        let mut offsite = meeting("offsite", 0, 24 * 60);
        offsite.all_day = true;
        let meetings = [declined, offsite, meeting("focus", 0, 5 * 60), meeting("hidden", 0, 30)];
        let hidden = BTreeSet::from(["hidden".to_owned()]);
        assert_eq!(phase(&meetings, 10 * MIN, 10 * MIN, &hidden), None);
    }

    #[test]
    fn nudges_once_at_the_start() {
        let meetings = [meeting("standup", 10 * MIN, 15)];
        let mut nudged = BTreeSet::new();
        assert!(starting(&meetings, 9 * MIN, &nudged).is_empty());
        assert_eq!(starting(&meetings, 10 * MIN + 5_000, &nudged).len(), 1);
        nudged.insert("standup".to_owned());
        assert!(starting(&meetings, 10 * MIN + 6_000, &nudged).is_empty());
        assert!(starting(&meetings, 13 * MIN, &BTreeSet::new()).is_empty(), "opening late doesn't nudge");
    }
}
