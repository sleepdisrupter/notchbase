//! Time zones. A known zone name comes from the system's database. Anything else, like the Windows
//! zone names Outlook sends, uses the definition inside the feed.

use jiff::Timestamp;
use jiff::civil::DateTime;
use jiff::tz::{Offset, TimeZone};

use super::component::Component;
use super::recur::Rule;
use super::stamp::{self, Stamp, Zone};

#[derive(Clone, Debug, PartialEq)]
struct Observance {
    start: DateTime,
    from: i32,
    to: i32,
    rule: Option<Rule>,
    extra: Vec<DateTime>,
}

#[derive(Clone, Debug, PartialEq)]
struct Defined {
    id: String,
    observances: Vec<Observance>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Zones {
    defined: Vec<Defined>,
}

enum Resolved<'a> {
    Known(TimeZone),
    Defined(&'a Defined),
}

impl Zones {
    pub fn read(calendar: &Component) -> Zones {
        Zones { defined: calendar.find("VTIMEZONE").filter_map(defined).collect() }
    }

    pub fn instant(&self, at: DateTime, zone: &Zone, local: &TimeZone) -> Option<Timestamp> {
        match self.resolve(zone, local) {
            Resolved::Known(tz) => tz.to_timestamp(at).ok(),
            Resolved::Defined(defined) => fixed(defined.offset_at(at))?.to_timestamp(at).ok(),
        }
    }

    pub fn civil(&self, instant: Timestamp, zone: &Zone, local: &TimeZone) -> DateTime {
        match self.resolve(zone, local) {
            Resolved::Known(tz) => instant.to_zoned(tz).datetime(),
            Resolved::Defined(defined) => {
                let guess = instant.to_zoned(TimeZone::UTC).datetime();
                let offset = defined.offset_at(guess);
                fixed(offset).map_or(guess, |tz| instant.to_zoned(tz).datetime())
            }
        }
    }

    pub fn stamp_instant(&self, stamp: &Stamp, local: &TimeZone) -> Option<Timestamp> {
        self.instant(stamp.at, &stamp.zone, local)
    }

    fn resolve<'a>(&'a self, zone: &Zone, local: &TimeZone) -> Resolved<'a> {
        let id = match zone {
            Zone::Utc => return Resolved::Known(TimeZone::UTC),
            Zone::Floating => return Resolved::Known(local.clone()),
            Zone::Named(id) => id,
        };
        if let Ok(tz) = TimeZone::get(id) {
            return Resolved::Known(tz);
        }
        if let Some(defined) = self.defined.iter().find(|defined| defined.id == *id) {
            return Resolved::Defined(defined);
        }
        match tail(id).and_then(|tail| TimeZone::get(tail).ok()) {
            Some(tz) => Resolved::Known(tz),
            None => Resolved::Known(local.clone()),
        }
    }
}

impl Defined {
    fn offset_at(&self, at: DateTime) -> i32 {
        let latest = self
            .observances
            .iter()
            .flat_map(|observance| observance.onsets(at).into_iter().map(move |onset| (onset, observance.to)))
            .filter(|(onset, _)| *onset <= at)
            .max_by_key(|(onset, _)| *onset);
        match latest {
            Some((_, to)) => to,
            None => self.observances.iter().min_by_key(|observance| observance.start).map_or(0, |first| first.from),
        }
    }
}

impl Observance {
    fn onsets(&self, at: DateTime) -> Vec<DateTime> {
        let horizon = at.tomorrow().unwrap_or(at);
        let mut onsets = match &self.rule {
            Some(rule) => rule.dates(self.start, rule.until.as_ref().map(|until| until.at), horizon),
            None => vec![self.start],
        };
        onsets.extend(self.extra.iter().copied());
        onsets
    }
}

fn defined(component: &Component) -> Option<Defined> {
    let id = component.text("TZID").trim().to_owned();
    let observances: Vec<Observance> = component
        .children
        .iter()
        .filter(|child| matches!(child.kind.as_str(), "STANDARD" | "DAYLIGHT"))
        .filter_map(observance)
        .collect();
    (!id.is_empty() && !observances.is_empty()).then_some(Defined { id, observances })
}

fn observance(component: &Component) -> Option<Observance> {
    let start = component.get("DTSTART").and_then(Stamp::from_property)?.at;
    let to = stamp::offset_of(&component.get("TZOFFSETTO")?.value)?;
    let from = component.get("TZOFFSETFROM").and_then(|prop| stamp::offset_of(&prop.value)).unwrap_or(to);
    let rule = component.get("RRULE").and_then(|prop| Rule::parse(&prop.value));
    let extra = component.all("RDATE").flat_map(Stamp::list).map(|stamp| stamp.at).collect();
    Some(Observance { start, from, to, rule, extra })
}

fn fixed(seconds: i32) -> Option<TimeZone> {
    Offset::from_seconds(seconds).ok().map(TimeZone::fixed)
}

fn tail(id: &str) -> Option<&str> {
    let trimmed = id.trim_matches('/');
    let (cut, _) = trimmed.rmatch_indices('/').nth(1)?;
    trimmed.get(cut + 1..)
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;

    use super::super::lines::properties;
    use super::*;

    const PACIFIC: &str = "BEGIN:VCALENDAR
BEGIN:VTIMEZONE
TZID:Pacific Standard Time
BEGIN:STANDARD
DTSTART:16010101T020000
TZOFFSETFROM:-0700
TZOFFSETTO:-0800
RRULE:FREQ=YEARLY;INTERVAL=1;BYDAY=1SU;BYMONTH=11
END:STANDARD
BEGIN:DAYLIGHT
DTSTART:16010101T020000
TZOFFSETFROM:-0800
TZOFFSETTO:-0700
RRULE:FREQ=YEARLY;INTERVAL=1;BYDAY=2SU;BYMONTH=3
END:DAYLIGHT
END:VTIMEZONE
END:VCALENDAR";

    fn zones(text: &str) -> Zones {
        let root = Component::tree(properties(text));
        Zones::read(root.find("VCALENDAR").next().unwrap())
    }

    fn named(id: &str) -> Zone {
        Zone::Named(id.into())
    }

    #[test]
    fn windows_zone_follows_own_rules() {
        let zones = zones(PACIFIC);
        let local = TimeZone::UTC;
        let summer = zones.instant(date(2026, 7, 1).at(9, 0, 0, 0), &named("Pacific Standard Time"), &local).unwrap();
        assert_eq!(summer.to_string(), "2026-07-01T16:00:00Z");
        let winter = zones.instant(date(2026, 12, 1).at(9, 0, 0, 0), &named("Pacific Standard Time"), &local).unwrap();
        assert_eq!(winter.to_string(), "2026-12-01T17:00:00Z");
    }

    #[test]
    fn iana_names_floating_and_utc_resolve() {
        let zones = zones("BEGIN:VCALENDAR\nEND:VCALENDAR");
        let local = TimeZone::get("Europe/Berlin").unwrap();
        let at = date(2026, 9, 30).at(9, 0, 0, 0);
        assert_eq!(zones.instant(at, &named("America/New_York"), &local).unwrap().to_string(), "2026-09-30T13:00:00Z");
        assert_eq!(zones.instant(at, &Zone::Floating, &local).unwrap().to_string(), "2026-09-30T07:00:00Z");
        assert_eq!(zones.instant(at, &Zone::Utc, &local).unwrap().to_string(), "2026-09-30T09:00:00Z");
        let mozilla = named("/mozilla.org/20050126_1/America/New_York");
        assert_eq!(zones.instant(at, &mozilla, &local).unwrap().to_string(), "2026-09-30T13:00:00Z");
        assert_eq!(
            zones.instant(at, &named("Nowhere Standard Time"), &local).unwrap().to_string(),
            "2026-09-30T07:00:00Z"
        );
    }

    #[test]
    fn civil_gives_wall_time() {
        let zones = zones(PACIFIC);
        let instant: Timestamp = "2026-07-01T16:00:00Z".parse().unwrap();
        let wall = zones.civil(instant, &named("Pacific Standard Time"), &TimeZone::UTC);
        assert_eq!(wall, date(2026, 7, 1).at(9, 0, 0, 0));
    }
}
