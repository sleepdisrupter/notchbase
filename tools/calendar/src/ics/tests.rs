use jiff::Timestamp;
use jiff::tz::TimeZone;

use super::*;

const GOOGLE: &str = "BEGIN:VCALENDAR\r
PRODID:-//Google Inc//Google Calendar 70.9054//EN\r
VERSION:2.0\r
X-WR-CALNAME:me@example.com\r
X-WR-TIMEZONE:America/New_York\r
BEGIN:VEVENT\r
DTSTART;TZID=America/New_York:20260921T093000\r
DTEND;TZID=America/New_York:20260921T094500\r
RRULE:FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR\r
EXDATE;TZID=America/New_York:20260929T093000\r
UID:standup@google.com\r
SUMMARY:Standup\r
DESCRIPTION:Join with Google Meet: https://meet.google.com/abc-defg-hij\r
END:VEVENT\r
BEGIN:VEVENT\r
DTSTART;TZID=America/New_York:20260930T110000\r
DTEND;TZID=America/New_York:20260930T111500\r
RECURRENCE-ID;TZID=America/New_York:20260930T093000\r
UID:standup@google.com\r
SUMMARY:Standup (moved)\r
END:VEVENT\r
BEGIN:VEVENT\r
DTSTART;TZID=America/New_York:20261001T093000\r
DTEND;TZID=America/New_York:20261001T094500\r
RECURRENCE-ID;TZID=America/New_York:20261001T093000\r
UID:standup@google.com\r
STATUS:CANCELLED\r
SUMMARY:Standup\r
END:VEVENT\r
BEGIN:VEVENT\r
DTSTART;VALUE=DATE:20260930\r
DTEND;VALUE=DATE:20261001\r
UID:holiday@google.com\r
SUMMARY:Company offsite\r
END:VEVENT\r
BEGIN:VEVENT\r
DTSTART:20260930T180000Z\r
DTEND:20260930T190000Z\r
UID:review@google.com\r
SUMMARY:Design review\r
ATTENDEE;PARTSTAT=DECLINED;CN=me@example.com:mailto:me@example.com\r
END:VEVENT\r
END:VCALENDAR\r
";

fn new_york() -> TimeZone {
    TimeZone::get("America/New_York").unwrap()
}

fn instant(text: &str) -> Timestamp {
    text.parse().unwrap()
}

fn meetings(text: &str, from: &str, to: &str) -> Vec<Meeting> {
    let mut found = Feed::parse(text).unwrap().meetings(instant(from), instant(to), &new_york());
    found.sort_by_key(|meeting| (meeting.start, meeting.title.clone()));
    found
}

fn local(ms: u64) -> String {
    Timestamp::from_millisecond(ms as i64).unwrap().to_zoned(new_york()).strftime("%a %H:%M").to_string()
}

#[test]
fn google_feed_expands() {
    let feed = Feed::parse(GOOGLE).unwrap();
    assert_eq!(feed.name, "me@example.com");
    let day = meetings(GOOGLE, "2026-09-30T04:00:00Z", "2026-10-01T04:00:00Z");
    let seen: Vec<(String, String, bool)> =
        day.iter().map(|meeting| (local(meeting.start), meeting.title.clone(), meeting.declined)).collect();
    assert_eq!(
        seen,
        [
            ("Wed 00:00".into(), "Company offsite".into(), false),
            ("Wed 11:00".into(), "Standup (moved)".into(), false),
            ("Wed 14:00".into(), "Design review".into(), true),
        ]
    );
    assert!(day[0].all_day);
    assert_eq!(day[0].end - day[0].start, 24 * 3_600_000);
}

#[test]
fn skipped_days_are_gone() {
    let week = meetings(GOOGLE, "2026-09-28T04:00:00Z", "2026-10-03T04:00:00Z");
    let standups: Vec<String> =
        week.iter().filter(|meeting| meeting.title == "Standup").map(|meeting| local(meeting.start)).collect();
    assert_eq!(standups, ["Mon 09:30", "Fri 09:30"]);
    let standup = week.iter().find(|meeting| meeting.title == "Standup").unwrap();
    assert_eq!(standup.join.as_deref(), Some("https://meet.google.com/abc-defg-hij"));
    assert_eq!(standup.end - standup.start, 15 * 60_000);
}

#[test]
fn weekly_keeps_wall_time_over_dst() {
    let text = "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:w\nSUMMARY:Weekly\nDTSTART;TZID=America/New_York:20261026T090000\nDURATION:PT1H\nRRULE:FREQ=WEEKLY\nEND:VEVENT\nEND:VCALENDAR";
    let found = meetings(text, "2026-10-25T00:00:00Z", "2026-11-10T00:00:00Z");
    let times: Vec<String> = found.iter().map(|meeting| local(meeting.start)).collect();
    assert_eq!(times, ["Mon 09:00", "Mon 09:00", "Mon 09:00"]);
    assert_eq!(found[1].start - found[0].start, (7 * 24 + 1) * 3_600_000);
}

#[test]
fn outlook_windows_zones_read() {
    let text = "BEGIN:VCALENDAR\nBEGIN:VTIMEZONE\nTZID:Eastern Standard Time\nBEGIN:STANDARD\nDTSTART:16010101T020000\nTZOFFSETFROM:-0400\nTZOFFSETTO:-0500\nRRULE:FREQ=YEARLY;INTERVAL=1;BYDAY=1SU;BYMONTH=11\nEND:STANDARD\nBEGIN:DAYLIGHT\nDTSTART:16010101T020000\nTZOFFSETFROM:-0500\nTZOFFSETTO:-0400\nRRULE:FREQ=YEARLY;INTERVAL=1;BYDAY=2SU;BYMONTH=3\nEND:DAYLIGHT\nEND:VTIMEZONE\nBEGIN:VEVENT\nUID:o\nSUMMARY:1:1\nDTSTART;TZID=Eastern Standard Time:20260930T140000\nDTEND;TZID=Eastern Standard Time:20260930T143000\nLOCATION:Microsoft Teams Meeting\nDESCRIPTION:Join: https://teams.microsoft.com/l/meetup-join/19%3ameeting\nEND:VEVENT\nEND:VCALENDAR";
    let found = meetings(text, "2026-09-30T04:00:00Z", "2026-10-01T04:00:00Z");
    assert_eq!(found.len(), 1);
    assert_eq!(local(found[0].start), "Wed 14:00");
    assert!(found[0].join.as_deref().is_some_and(|link| link.contains("teams.microsoft.com")));
}

#[test]
fn web_page_is_not_a_calendar() {
    assert_eq!(Feed::parse("<html><body>Sign in</body></html>"), Err(NOT_A_CALENDAR.to_owned()));
}

#[test]
fn apple_colors_are_read() {
    let feed = Feed::parse("BEGIN:VCALENDAR\nX-APPLE-CALENDAR-COLOR:#FF2968FF\nEND:VCALENDAR").unwrap();
    assert_eq!(feed.color, Some(Color::rgb(0xFF, 0x29, 0x68)));
}
