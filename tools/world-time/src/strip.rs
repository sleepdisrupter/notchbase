//! One moment read in a city: the hours on its band, its clock, date and abbreviation, and the gap from
//! home.

use jiff::tz::TimeZone;
use jiff::{Timestamp, Zoned};

pub const HOUR_MS: i64 = 3_600_000;
const MINUTE_MS: i64 = 60_000;
const SECOND_MS: i64 = 1_000;
const MINUS: char = '\u{2212}'; // a real minus sign, wider than a hyphen
const DAYLIGHT: [i8; 2] = [8, 17]; // hours drawn bright
const WAKING: [i8; 2] = [6, 21]; // dimmer, and the rest is night

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Light {
    Day,
    Dusk,
    Night,
    NewDay,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hour {
    pub top: String,
    pub bottom: String,
    pub light: Light,
}

pub fn hour_start(now: u64, home: &TimeZone) -> i64 {
    let now = now.min(i64::MAX as u64) as i64;
    let local = local(now, home);
    now - i64::from(local.minute()) * MINUTE_MS - i64::from(local.second()) * SECOND_MS - i64::from(local.millisecond())
}

pub fn hours(center: i64, each_side: i64, zone: &TimeZone, full_day: bool) -> Vec<Hour> {
    (-each_side..=each_side).map(|step| hour(center + step * HOUR_MS, zone, full_day)).collect()
}

pub fn hour(at: i64, zone: &TimeZone, full_day: bool) -> Hour {
    let local = local(at, zone);
    // Midnight shows the date instead of the hour.
    if local.hour() == 0 {
        let month = local.strftime("%b").to_string().to_uppercase();
        return Hour { top: month, bottom: local.day().to_string(), light: Light::NewDay };
    }
    let top = if full_day { local.hour() } else { twelve(local.hour()) }.to_string();
    let bottom = match (local.minute(), full_day) {
        (0, false) => half(local.hour()).to_lowercase(),
        (minute, _) => format!(":{minute:02}"),
    };
    Hour { top, bottom, light: light(local.hour()) }
}

pub fn time(at: i64, zone: &TimeZone, full_day: bool) -> String {
    let (clock, half) = clock(at, zone, full_day);
    if half.is_empty() { clock } else { format!("{clock} {half}") }
}

pub fn clock(at: i64, zone: &TimeZone, full_day: bool) -> (String, &'static str) {
    let local = local(at, zone);
    let (hour, minute) = (local.hour(), local.minute());
    if full_day {
        (format!("{hour:02}:{minute:02}"), "")
    } else {
        (format!("{}:{minute:02}", twelve(hour)), half(hour))
    }
}

pub fn date(at: i64, zone: &TimeZone) -> String {
    local(at, zone).strftime("%a %-d").to_string()
}

pub fn abbreviation(at: i64, zone: &TimeZone) -> String {
    zone.to_offset_info(stamp(at)).abbreviation().to_owned()
}

pub fn gap(at: i64, home: &TimeZone, zone: &TimeZone) -> Option<String> {
    let ahead = |zone: &TimeZone| i64::from(zone.to_offset(stamp(at)).seconds()) / 60;
    let minutes = ahead(zone) - ahead(home);
    let sign = match minutes {
        0 => return None,
        minutes if minutes > 0 => '+',
        _ => MINUS,
    };
    let (hours, minutes) = (minutes.abs() / 60, minutes.abs() % 60);
    Some(if minutes == 0 { format!("{sign}{hours}") } else { format!("{sign}{hours}:{minutes:02}") })
}

fn local(at: i64, zone: &TimeZone) -> Zoned {
    stamp(at).to_zoned(zone.clone())
}

fn stamp(at: i64) -> Timestamp {
    Timestamp::from_millisecond(at).unwrap_or(Timestamp::UNIX_EPOCH)
}

fn twelve(hour: i8) -> i8 {
    (hour + 11) % 12 + 1
}

fn half(hour: i8) -> &'static str {
    if hour < 12 { "AM" } else { "PM" }
}

fn light(hour: i8) -> Light {
    let within = |[from, to]: [i8; 2]| (from..=to).contains(&hour);
    if within(DAYLIGHT) {
        Light::Day
    } else if within(WAKING) {
        Light::Dusk
    } else {
        Light::Night
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zones::find;

    const NOON_IN_LONDON: i64 = 1_785_927_960_000;

    fn zone(name: &str) -> TimeZone {
        find(name).expect("a zone")
    }

    fn tops(hours: &[Hour]) -> Vec<&str> {
        hours.iter().map(|hour| hour.top.as_str()).collect()
    }

    #[test]
    fn moment_reads_per_city() {
        assert_eq!(time(NOON_IN_LONDON, &zone("London"), false), "12:06 PM");
        assert_eq!(time(NOON_IN_LONDON, &zone("New York"), false), "7:06 AM");
        assert_eq!(time(NOON_IN_LONDON, &zone("San Francisco"), false), "4:06 AM");
        assert_eq!(time(NOON_IN_LONDON, &zone("Tokyo"), true), "20:06");
        assert_eq!(clock(NOON_IN_LONDON, &zone("New York"), false), ("7:06".to_owned(), "AM"));
        assert_eq!(date(NOON_IN_LONDON, &zone("London")), "Wed 5");
        assert_eq!(abbreviation(NOON_IN_LONDON, &zone("London")), "BST");
        assert_eq!(abbreviation(NOON_IN_LONDON, &zone("New York")), "EDT");
    }

    #[test]
    fn gap_counts_from_home() {
        let home = zone("London");
        let minus = |hours: &str| format!("{MINUS}{hours}");
        assert_eq!(gap(NOON_IN_LONDON, &home, &zone("New York")), Some(minus("5")));
        assert_eq!(gap(NOON_IN_LONDON, &home, &zone("San Francisco")), Some(minus("8")));
        assert_eq!(gap(NOON_IN_LONDON, &home, &zone("Tokyo")).as_deref(), Some("+8"));
        assert_eq!(gap(NOON_IN_LONDON, &home, &zone("Kolkata")).as_deref(), Some("+4:30"));
        assert_eq!(gap(NOON_IN_LONDON, &home, &zone("Edinburgh")), None);
    }

    #[test]
    fn gap_follows_daylight_saving() {
        let winter = NOON_IN_LONDON + 150 * 24 * HOUR_MS;
        assert_eq!(gap(winter, &zone("London"), &zone("Tokyo")).as_deref(), Some("+9"));
    }

    #[test]
    fn regions_change_on_own_dates() {
        let at = |date: &str| format!("{date}T12:00:00Z").parse::<Timestamp>().expect("a moment").as_millisecond();
        let (london, new_york) = (zone("Europe/London"), zone("America/New York"));
        let read = |date: &str| {
            let at = at(date);
            (abbreviation(at, &london), abbreviation(at, &new_york), gap(at, &london, &new_york).expect("a gap"))
        };
        let behind = |hours: u8| format!("{MINUS}{hours}");
        assert_eq!(read("2026-10-20"), ("BST".into(), "EDT".into(), behind(5)));
        assert_eq!(read("2026-10-28"), ("GMT".into(), "EDT".into(), behind(4)));
        assert_eq!(read("2026-11-05"), ("GMT".into(), "EST".into(), behind(5)));
        assert_eq!(read("2027-03-20"), ("GMT".into(), "EDT".into(), behind(4)));
        assert_eq!(read("2027-04-02"), ("BST".into(), "EDT".into(), behind(5)));
    }

    #[test]
    fn strip_starts_on_the_hour() {
        let start = hour_start(NOON_IN_LONDON as u64, &zone("London"));
        assert_eq!(time(start, &zone("London"), false), "12:00 PM");
        assert_eq!(time(hour_start(NOON_IN_LONDON as u64, &zone("Kolkata")), &zone("Kolkata"), false), "4:00 PM");
    }

    #[test]
    fn midnight_shows_the_date() {
        let start = hour_start(NOON_IN_LONDON as u64, &zone("London"));
        let london = hours(start, 2, &zone("London"), false);
        assert_eq!(tops(&london), ["10", "11", "12", "1", "2"]);
        assert_eq!(london[1].bottom, "am");
        assert_eq!(london[2].bottom, "pm");
        let tokyo = hours(start, 4, &zone("Tokyo"), false);
        assert_eq!(tops(&tokyo), ["4", "5", "6", "7", "8", "9", "10", "11", "AUG"]);
        assert_eq!(tokyo[8], Hour { top: "AUG".into(), bottom: "6".into(), light: Light::NewDay });
    }

    #[test]
    fn half_hour_zones_show_minutes() {
        let start = hour_start(NOON_IN_LONDON as u64, &zone("London"));
        let kolkata = hour(start, &zone("Kolkata"), false);
        assert_eq!((kolkata.top.as_str(), kolkata.bottom.as_str()), ("4", ":30"));
        let london = hour(start + HOUR_MS, &zone("London"), true);
        assert_eq!((london.top.as_str(), london.bottom.as_str()), ("13", ":00"));
    }

    #[test]
    fn hours_lit_by_time_of_day() {
        let lights: Vec<Light> = [3, 6, 8, 17, 18, 21, 22].into_iter().map(light).collect();
        assert_eq!(lights, [Light::Night, Light::Dusk, Light::Day, Light::Day, Light::Dusk, Light::Dusk, Light::Night]);
    }
}
