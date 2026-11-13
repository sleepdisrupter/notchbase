//! Date and time values: whole days, UTC, floating, or in a named zone.

use jiff::civil::{Date, DateTime, Time};

use super::lines::Property;

#[derive(Clone, Debug, PartialEq)]
pub enum Zone {
    Utc,
    Floating,
    Named(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stamp {
    pub at: DateTime,
    pub all_day: bool,
    pub zone: Zone,
}

impl Stamp {
    pub fn from_property(prop: &Property) -> Option<Stamp> {
        let tzid = prop.param("TZID").map(str::to_owned);
        let first = prop.value.split(',').next()?;
        parse(first, tzid.as_deref(), prop.param("VALUE") == Some("DATE"))
    }

    pub fn list(prop: &Property) -> Vec<Stamp> {
        let tzid = prop.param("TZID");
        let dates = prop.param("VALUE") == Some("DATE");
        prop.value.split(',').filter_map(|value| parse(value, tzid, dates)).collect()
    }
}

pub fn parse(value: &str, tzid: Option<&str>, date_only: bool) -> Option<Stamp> {
    let value = value.trim();
    let (date, rest) = value.split_at_checked(8)?;
    let day = date_of(date)?;
    if date_only || rest.is_empty() {
        return Some(Stamp { at: day.to_datetime(Time::midnight()), all_day: true, zone: Zone::Floating });
    }
    let clock = rest.strip_prefix('T')?;
    let (clock, utc) = match clock.strip_suffix(['Z', 'z']) {
        Some(clock) => (clock, true),
        None => (clock, false),
    };
    let at = day.to_datetime(time_of(clock)?);
    let zone = match (utc, tzid) {
        (true, _) => Zone::Utc,
        (false, Some(tzid)) if !tzid.trim().is_empty() => Zone::Named(tzid.trim().to_owned()),
        _ => Zone::Floating,
    };
    Some(Stamp { at, all_day: false, zone })
}

pub fn date_of(text: &str) -> Option<Date> {
    if text.len() != 8 || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let number = |range: std::ops::Range<usize>| text[range].parse::<i32>().ok();
    Date::new(i16::try_from(number(0..4)?).ok()?, i8::try_from(number(4..6)?).ok()?, i8::try_from(number(6..8)?).ok()?)
        .ok()
}

fn time_of(text: &str) -> Option<Time> {
    if text.len() < 4 || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let number = |range: std::ops::Range<usize>| text.get(range).map_or(Some(0), |part| part.parse::<i8>().ok());
    let second = number(4..6)?.min(59);
    Time::new(number(0..2)?, number(2..4)?, second, 0).ok()
}

pub fn offset_of(text: &str) -> Option<i32> {
    let text = text.trim();
    let (sign, digits) = match text.split_at_checked(1)? {
        ("+", digits) => (1, digits),
        ("-", digits) => (-1, digits),
        _ => return None,
    };
    let number = |range: std::ops::Range<usize>| digits.get(range).map_or(Some(0), |part| part.parse::<i32>().ok());
    Some(sign * (number(0..2)? * 3600 + number(2..4)? * 60 + number(4..6)?))
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;

    use super::*;

    #[test]
    fn stamps_read_every_kind() {
        let all_day = parse("20260930", None, true).unwrap();
        assert!(all_day.all_day);
        assert_eq!(all_day.at, date(2026, 9, 30).at(0, 0, 0, 0));
        let utc = parse("20260930T143000Z", Some("Europe/Paris"), false).unwrap();
        assert_eq!((utc.zone, utc.at), (Zone::Utc, date(2026, 9, 30).at(14, 30, 0, 0)));
        let zoned = parse("20260930T090000", Some("America/New_York"), false).unwrap();
        assert_eq!(zoned.zone, Zone::Named("America/New_York".into()));
        assert_eq!(parse("20260930T0900", None, false).unwrap().zone, Zone::Floating);
        assert!(parse("2026-09-30", None, false).is_none());
    }

    #[test]
    fn offsets_read_in_seconds() {
        assert_eq!(offset_of("-0700"), Some(-7 * 3600));
        assert_eq!(offset_of("+0530"), Some(5 * 3600 + 30 * 60));
        assert_eq!(offset_of("0100"), None);
    }
}
