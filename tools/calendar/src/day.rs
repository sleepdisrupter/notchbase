//! Day boundaries and day labels in local time.

use jiff::civil::{Date, DateTime, Time};
use jiff::tz::TimeZone;
use jiff::{Span, Timestamp};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Days {
    pub today: Date,
    pub today_ms: u64,
    pub tomorrow_ms: u64,
    pub after_ms: u64,
}

impl Days {
    pub fn around(now: u64, zone: &TimeZone) -> Days {
        let today = local(now, zone).date();
        let start = |day: Date| midnight(day, zone);
        let tomorrow = shift(today, 1);
        Days { today, today_ms: start(today), tomorrow_ms: start(tomorrow), after_ms: start(shift(today, 2)) }
    }
}

pub fn local(ms: u64, zone: &TimeZone) -> DateTime {
    Timestamp::from_millisecond(ms as i64).unwrap_or(Timestamp::UNIX_EPOCH).to_zoned(zone.clone()).datetime()
}

pub fn midnight(day: Date, zone: &TimeZone) -> u64 {
    at(day, Time::midnight(), zone)
}

pub fn at(day: Date, time: Time, zone: &TimeZone) -> u64 {
    zone.to_timestamp(day.to_datetime(time)).map_or(0, |at| u64::try_from(at.as_millisecond()).unwrap_or(0))
}

pub fn shift(day: Date, days: i64) -> Date {
    Span::new().try_days(days).ok().and_then(|span| day.checked_add(span).ok()).unwrap_or(day)
}

pub fn week_title(first: Date, last: Date) -> String {
    let side = |day: Date| format!("{} {}", &month(day)[..3], day.day());
    format!("{} \u{2013} {}", side(first), side(last))
}

pub fn short_weekday(day: Date) -> &'static str {
    &weekday(day)[..3]
}

pub fn short_time(ms: u64, zone: &TimeZone, clock_24h: bool) -> String {
    let at = local(ms, zone);
    let (hour, minute) = (at.hour(), at.minute());
    if clock_24h {
        return format!("{hour}:{minute:02}");
    }
    let half = if hour < 12 { "a" } else { "p" };
    let hour = match hour % 12 {
        0 => 12,
        hour => hour,
    };
    match minute {
        0 => format!("{hour}{half}"),
        minute => format!("{hour}:{minute:02}{half}"),
    }
}

pub fn month_title(day: Date) -> String {
    format!("{} {}", month(day), day.year())
}

pub fn heading(day: Date) -> String {
    format!("{}, {} {}", weekday(day), month(day), day.day())
}

pub fn named(day: Date, today: Date) -> String {
    match day.since(today).map(|span| span.get_days()) {
        Ok(0) => "Today".to_owned(),
        Ok(1) => "Tomorrow".to_owned(),
        Ok(-1) => "Yesterday".to_owned(),
        _ => format!("{}, {} {}", &weekday(day)[..3], &month(day)[..3], day.day()),
    }
}

fn weekday(day: Date) -> &'static str {
    ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"]
        [day.weekday().to_monday_zero_offset() as usize]
}

fn month(day: Date) -> &'static str {
    [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ][(day.month() - 1) as usize]
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;

    use super::*;

    #[test]
    fn days_start_at_local_midnight() {
        let zone = TimeZone::get("America/New_York").unwrap();
        let noon = at(date(2026, 9, 30), Time::new(12, 0, 0, 0).unwrap(), &zone);
        let days = Days::around(noon, &zone);
        assert_eq!(days.today, date(2026, 9, 30));
        assert_eq!(days.tomorrow_ms - days.today_ms, 24 * 3_600_000);
        assert_eq!(local(days.today_ms, &zone), date(2026, 9, 30).at(0, 0, 0, 0));
    }

    #[test]
    fn clock_change_day_is_longer() {
        let zone = TimeZone::get("America/New_York").unwrap();
        let days = Days::around(at(date(2026, 11, 1), Time::new(9, 0, 0, 0).unwrap(), &zone), &zone);
        assert_eq!(days.tomorrow_ms - days.today_ms, 25 * 3_600_000);
    }

    #[test]
    fn labels_read_naturally() {
        let today = date(2026, 9, 30);
        assert_eq!(heading(today), "Wednesday, September 30");
        assert_eq!(named(today, today), "Today");
        assert_eq!(named(shift(today, 1), today), "Tomorrow");
        assert_eq!(named(shift(today, -1), today), "Yesterday");
        assert_eq!(named(shift(today, 2), today), "Fri, Oct 2");
        assert_eq!(week_title(date(2026, 9, 28), date(2026, 10, 4)), "Sep 28 \u{2013} Oct 4");
        assert_eq!(short_weekday(today), "Wed");
    }

    #[test]
    fn short_times_fit_on_a_chip() {
        let zone = TimeZone::UTC;
        let at = |hour, minute| super::at(date(2026, 9, 30), Time::new(hour, minute, 0, 0).unwrap(), &zone);
        assert_eq!(short_time(at(9, 30), &zone, false), "9:30a");
        assert_eq!(short_time(at(14, 0), &zone, false), "2p");
        assert_eq!(short_time(at(0, 15), &zone, false), "12:15a");
        assert_eq!(short_time(at(14, 0), &zone, true), "14:00");
    }
}
