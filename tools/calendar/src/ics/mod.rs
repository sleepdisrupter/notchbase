//! A small iCalendar (.ics) reader. It handles what Google, Outlook and Apple feeds send: recurring
//! events, exceptions and the feed's own time zones.

mod component;
mod lines;
mod recur;
mod series;
mod stamp;
mod zones;

use std::collections::HashSet;

use jiff::civil::DateTime;
use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp};
use notchbase_sdk::protocol::Color;

use crate::join;
use crate::meeting::{Meeting, named};
use component::Component;
use lines::properties;
use recur::add_days;
use series::Series;
use stamp::Stamp;
use zones::Zones;

pub const NOT_A_CALENDAR: &str = "That link isn't a calendar";

#[derive(Clone, Debug, PartialEq)]
pub struct Feed {
    pub name: String,
    pub color: Option<Color>,
    series: Vec<Series>,
    zones: Zones,
}

struct Window<'a> {
    from: Timestamp,
    to: Timestamp,
    local: &'a TimeZone,
}

impl Feed {
    pub fn parse(text: &str) -> Result<Feed, String> {
        let root = Component::tree(properties(text));
        let calendar = root.find("VCALENDAR").next().ok_or_else(|| NOT_A_CALENDAR.to_owned())?;
        let name = calendar.text("X-WR-CALNAME").trim().to_owned();
        let owner = name.contains('@').then_some(name.as_str());
        let series =
            calendar.descendants("VEVENT").into_iter().filter_map(|event| Series::read(event, owner)).collect();
        let color = calendar.get("X-APPLE-CALENDAR-COLOR").and_then(|prop| hex(&prop.value));
        Ok(Feed { name, color, series, zones: Zones::read(calendar) })
    }

    pub fn meetings(&self, from: Timestamp, to: Timestamp, local: &TimeZone) -> Vec<Meeting> {
        let window = Window { from, to, local };
        let moved: HashSet<(&str, i64)> = self
            .series
            .iter()
            .filter_map(|series| {
                let from = series.moved_from.as_ref()?;
                Some((series.uid.as_str(), self.zones.stamp_instant(from, local)?.as_millisecond()))
            })
            .collect();
        self.series
            .iter()
            .filter(|series| !series.cancelled)
            .flat_map(|series| {
                let spans = match series.moved_from {
                    Some(_) => self.single(series, &window),
                    None => self.repeated(series, &window, &moved),
                };
                spans.into_iter().map(move |(start, end)| self.meeting(series, start, end))
            })
            .collect()
    }

    fn single(&self, series: &Series, window: &Window) -> Vec<(Timestamp, Timestamp)> {
        self.span(series, series.start.at, window).filter(|span| overlaps(*span, window)).into_iter().collect()
    }

    fn repeated(&self, series: &Series, window: &Window, moved: &HashSet<(&str, i64)>) -> Vec<(Timestamp, Timestamp)> {
        let zone = &series.start.zone;
        let horizon = add_days(self.zones.civil(window.to, zone, window.local).date(), 2)
            .map_or(DateTime::MAX, |day| day.to_datetime(jiff::civil::Time::midnight()));
        let mut starts = match &series.rule {
            Some(rule) => rule.dates(series.start.at, self.until(series, window.local), horizon),
            None => vec![series.start.at],
        };
        starts.extend(series.extra.iter().filter_map(|extra| self.civil_in(extra, zone, window.local)));
        let skipped: HashSet<i64> = series
            .skipped
            .iter()
            .filter_map(|stamp| self.civil_in(stamp, zone, window.local))
            .filter_map(|at| self.zones.instant(at, zone, window.local))
            .map(|at| at.as_millisecond())
            .collect();
        starts.sort();
        starts.dedup();
        starts
            .into_iter()
            .filter_map(|at| self.span(series, at, window))
            .filter(|(start, _)| {
                let key = start.as_millisecond();
                !skipped.contains(&key) && !moved.contains(&(series.uid.as_str(), key))
            })
            .filter(|span| overlaps(*span, window))
            .collect()
    }

    fn span(&self, series: &Series, at: DateTime, window: &Window) -> Option<(Timestamp, Timestamp)> {
        let zone = &series.start.zone;
        let start = self.zones.instant(at, zone, window.local)?;
        if series.start.all_day {
            let days = series
                .end
                .as_ref()
                .and_then(|end| end.at.date().since(series.start.at.date()).ok())
                .map(|span| i64::from(span.get_days()))
                .or_else(|| series.length.map(|length| length.as_hours() / 24))
                .unwrap_or(1)
                .max(1);
            let end = add_days(at.date(), days)?.to_datetime(at.time());
            return Some((start, self.zones.instant(end, zone, window.local)?));
        }
        let length = match (&series.end, series.length) {
            (Some(end), _) => self
                .zones
                .stamp_instant(end, window.local)?
                .duration_since(self.zones.stamp_instant(&series.start, window.local)?),
            (None, Some(length)) => length,
            (None, None) => SignedDuration::ZERO,
        };
        Some((start, start.checked_add(length.max(SignedDuration::ZERO)).ok()?))
    }

    fn until(&self, series: &Series, local: &TimeZone) -> Option<DateTime> {
        let until = series.rule.as_ref()?.until.as_ref()?;
        self.civil_in(until, &series.start.zone, local)
    }

    fn civil_in(&self, stamp: &Stamp, zone: &stamp::Zone, local: &TimeZone) -> Option<DateTime> {
        if stamp.all_day || stamp.zone == *zone {
            return Some(stamp.at);
        }
        Some(self.zones.civil(self.zones.stamp_instant(stamp, local)?, zone, local))
    }

    fn meeting(&self, series: &Series, start: Timestamp, end: Timestamp) -> Meeting {
        let start_ms = millis(start);
        Meeting {
            id: format!("{}@{start_ms}", series.uid),
            title: named(&series.title),
            start: start_ms,
            end: millis(end).max(start_ms),
            all_day: series.start.all_day,
            location: series.location.trim().to_owned(),
            join: join::find(&[&series.conference, &series.url, &series.location, &series.notes]),
            link: series.url.trim().to_owned(),
            calendar: self.name.clone(),
            color: self.color,
            declined: series.declined,
            calendar_id: String::new(),
            editable: false,
        }
    }
}

fn overlaps((start, end): (Timestamp, Timestamp), window: &Window) -> bool {
    start < window.to && end.max(start + SignedDuration::from_millis(1)) > window.from
}

fn millis(at: Timestamp) -> u64 {
    u64::try_from(at.as_millisecond()).unwrap_or(0)
}

fn hex(text: &str) -> Option<Color> {
    let digits = text.trim().trim_start_matches('#');
    let channel = |at: usize| digits.get(at..at + 2).and_then(|pair| u8::from_str_radix(pair, 16).ok());
    (digits.len() == 6 || digits.len() == 8).then(|| Some(Color::rgb(channel(0)?, channel(2)?, channel(4)?))).flatten()
}

#[cfg(test)]
mod tests;
