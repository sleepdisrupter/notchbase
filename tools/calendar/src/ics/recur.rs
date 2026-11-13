//! Expands RRULE recurrence rules into dates.

use jiff::Span;
use jiff::civil::{Date, DateTime, Weekday};

use super::stamp::{self, Stamp};

const MOST_PERIODS: i64 = 40_000; // keeps a rule that never ends from running forever

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Freq {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Rule {
    pub freq: Freq,
    pub interval: i64,
    pub count: Option<u32>,
    pub until: Option<Stamp>,
    pub by_day: Vec<(i8, Weekday)>,
    pub by_month_day: Vec<i8>,
    pub by_month: Vec<i8>,
    pub by_set_pos: Vec<i32>,
    pub week_start: Weekday,
}

impl Rule {
    pub fn parse(value: &str) -> Option<Rule> {
        let mut rule = Rule {
            freq: Freq::Daily,
            interval: 1,
            count: None,
            until: None,
            by_day: Vec::new(),
            by_month_day: Vec::new(),
            by_month: Vec::new(),
            by_set_pos: Vec::new(),
            week_start: Weekday::Monday,
        };
        let mut freq = None;
        for part in value.trim().split(';') {
            let Some((key, value)) = part.split_once('=') else { continue };
            let list = || value.split(',').map(str::trim).filter(|item| !item.is_empty());
            match key.trim().to_ascii_uppercase().as_str() {
                "FREQ" => freq = frequency(value),
                "INTERVAL" => rule.interval = value.trim().parse::<i64>().ok().filter(|n| *n > 0)?,
                "COUNT" => rule.count = value.trim().parse().ok(),
                "UNTIL" => rule.until = stamp::parse(value, None, value.trim().len() == 8),
                "BYDAY" => rule.by_day = list().filter_map(ordinal_day).collect(),
                "BYMONTHDAY" => rule.by_month_day = list().filter_map(|day| day.parse().ok()).collect(),
                "BYMONTH" => rule.by_month = list().filter_map(|month| month.parse().ok()).collect(),
                "BYSETPOS" => rule.by_set_pos = list().filter_map(|pos| pos.parse().ok()).collect(),
                "WKST" => rule.week_start = weekday(value.trim()).unwrap_or(Weekday::Monday),
                _ => {}
            }
        }
        rule.freq = freq?;
        Some(rule)
    }

    pub fn dates(&self, start: DateTime, until: Option<DateTime>, horizon: DateTime) -> Vec<DateTime> {
        let mut out = Vec::new();
        let mut produced = 0u32;
        for period in 0..MOST_PERIODS {
            let Some((base, mut days)) = self.period(start.date(), period) else { break };
            if base > horizon.date() {
                break;
            }
            days.sort();
            days.dedup();
            for day in self.positions(days) {
                let at = day.to_datetime(start.time());
                if at < start {
                    continue;
                }
                if until.is_some_and(|until| at > until) || self.count.is_some_and(|count| produced >= count) {
                    return out;
                }
                if at >= horizon {
                    return out;
                }
                produced += 1;
                out.push(at);
            }
        }
        out
    }

    fn period(&self, start: Date, period: i64) -> Option<(Date, Vec<Date>)> {
        let step = period.checked_mul(self.interval)?;
        match self.freq {
            Freq::Daily => {
                let day = add_days(start, step)?;
                let keep = self.in_months(day) && self.on_weekday(day) && self.on_month_day(day);
                Some((day, if keep { vec![day] } else { Vec::new() }))
            }
            Freq::Weekly => {
                let week_start = self.week_start.to_monday_zero_offset();
                let back = (start.weekday().to_monday_zero_offset() - week_start).rem_euclid(7);
                let week = add_days(start, step.checked_mul(7)? - i64::from(back))?;
                let weekdays = match self.by_day.is_empty() {
                    true => vec![start.weekday()],
                    false => self.by_day.iter().map(|(_, weekday)| *weekday).collect(),
                };
                let days = weekdays
                    .into_iter()
                    .filter_map(|weekday| {
                        let ahead = (weekday.to_monday_zero_offset() - week_start).rem_euclid(7);
                        add_days(week, i64::from(ahead))
                    })
                    .filter(|day| self.in_months(*day))
                    .collect();
                Some((week, days))
            }
            Freq::Monthly => {
                let first = add_months(start.first_of_month(), step)?;
                let days = if self.in_months(first) { self.month_days(first, start.day()) } else { Vec::new() };
                Some((first, days))
            }
            Freq::Yearly => {
                let year = i16::try_from(i64::from(start.year()).checked_add(step)?).ok()?;
                let first = Date::new(year, 1, 1).ok()?;
                Some((first, self.year_days(year, start)))
            }
        }
    }

    fn year_days(&self, year: i16, start: Date) -> Vec<Date> {
        if self.by_month.is_empty() && self.by_month_day.is_empty() && !self.by_day.is_empty() {
            return self.by_day.iter().flat_map(|(nth, weekday)| year_weekdays(year, *nth, *weekday)).collect();
        }
        let months = match self.by_month.is_empty() {
            true => vec![start.month()],
            false => self.by_month.clone(),
        };
        months
            .into_iter()
            .filter_map(|month| Date::new(year, month, 1).ok())
            .flat_map(|first| self.month_days(first, start.day()))
            .collect()
    }

    fn month_days(&self, first: Date, default_day: i8) -> Vec<Date> {
        let length = first.days_in_month();
        if !self.by_month_day.is_empty() {
            return self
                .by_month_day
                .iter()
                .filter_map(|day| {
                    let day = if *day > 0 { *day } else { length + *day + 1 };
                    (1..=length).contains(&day).then(|| Date::new(first.year(), first.month(), day).ok()).flatten()
                })
                .filter(|day| self.on_weekday(*day))
                .collect();
        }
        if !self.by_day.is_empty() {
            return self
                .by_day
                .iter()
                .flat_map(|(nth, weekday)| match nth {
                    0 => every_weekday(first, *weekday),
                    nth => first.nth_weekday_of_month(*nth, *weekday).ok().into_iter().collect(),
                })
                .collect();
        }
        Date::new(first.year(), first.month(), default_day).ok().into_iter().collect()
    }

    fn positions(&self, days: Vec<Date>) -> Vec<Date> {
        if self.by_set_pos.is_empty() {
            return days;
        }
        let length = days.len() as i32;
        let mut picked: Vec<Date> = self
            .by_set_pos
            .iter()
            .filter_map(|pos| {
                let index = if *pos > 0 { pos - 1 } else { length + pos };
                usize::try_from(index).ok().and_then(|index| days.get(index).copied())
            })
            .collect();
        picked.sort();
        picked.dedup();
        picked
    }

    fn in_months(&self, day: Date) -> bool {
        self.by_month.is_empty() || self.by_month.contains(&day.month())
    }

    fn on_weekday(&self, day: Date) -> bool {
        self.by_day.is_empty() || self.by_day.iter().any(|(_, weekday)| *weekday == day.weekday())
    }

    fn on_month_day(&self, day: Date) -> bool {
        let length = day.days_in_month();
        self.by_month_day.is_empty()
            || self.by_month_day.iter().any(|wanted| {
                let wanted = if *wanted > 0 { *wanted } else { length + *wanted + 1 };
                wanted == day.day()
            })
    }
}

fn frequency(value: &str) -> Option<Freq> {
    match value.trim().to_ascii_uppercase().as_str() {
        "DAILY" => Some(Freq::Daily),
        "WEEKLY" => Some(Freq::Weekly),
        "MONTHLY" => Some(Freq::Monthly),
        "YEARLY" => Some(Freq::Yearly),
        _ => None,
    }
}

fn ordinal_day(text: &str) -> Option<(i8, Weekday)> {
    let split = text.len().checked_sub(2)?;
    let (nth, code) = text.split_at_checked(split)?;
    let nth = match nth.trim_start_matches('+') {
        "" => 0,
        nth => nth.parse().ok()?,
    };
    Some((nth, weekday(code)?))
}

fn weekday(code: &str) -> Option<Weekday> {
    match code.to_ascii_uppercase().as_str() {
        "MO" => Some(Weekday::Monday),
        "TU" => Some(Weekday::Tuesday),
        "WE" => Some(Weekday::Wednesday),
        "TH" => Some(Weekday::Thursday),
        "FR" => Some(Weekday::Friday),
        "SA" => Some(Weekday::Saturday),
        "SU" => Some(Weekday::Sunday),
        _ => None,
    }
}

fn every_weekday(first: Date, weekday: Weekday) -> Vec<Date> {
    (1..=5).filter_map(|nth| first.nth_weekday_of_month(nth, weekday).ok()).collect()
}

fn year_weekdays(year: i16, nth: i8, weekday: Weekday) -> Vec<Date> {
    let (Ok(first), Ok(last)) = (Date::new(year, 1, 1), Date::new(year, 12, 31)) else { return Vec::new() };
    let found = match nth {
        0 => {
            return (1..=12)
                .filter_map(|month| Date::new(year, month, 1).ok())
                .flat_map(|month| every_weekday(month, weekday))
                .collect();
        }
        nth if nth > 0 => first.yesterday().ok().and_then(|eve| eve.nth_weekday(i32::from(nth), weekday).ok()),
        nth => last.tomorrow().ok().and_then(|after| after.nth_weekday(i32::from(nth), weekday).ok()),
    };
    found.into_iter().filter(|day| day.year() == year).collect()
}

pub fn add_days(day: Date, days: i64) -> Option<Date> {
    day.checked_add(Span::new().try_days(days).ok()?).ok()
}

fn add_months(first: Date, months: i64) -> Option<Date> {
    let total = i64::from(first.year()) * 12 + i64::from(first.month() - 1) + months;
    Date::new(i16::try_from(total.div_euclid(12)).ok()?, (total.rem_euclid(12) + 1) as i8, 1).ok()
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;

    use super::*;

    fn run(rule: &str, start: DateTime, until: Option<DateTime>, horizon: DateTime) -> Vec<DateTime> {
        Rule::parse(rule).expect("a rule").dates(start, until, horizon)
    }

    fn days(dates: &[DateTime]) -> Vec<(i8, i8)> {
        dates.iter().map(|at| (at.month(), at.day())).collect()
    }

    #[test]
    fn weekday_standup_skips_weekend() {
        let start = date(2026, 9, 28).at(9, 30, 0, 0);
        let got = run("FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR", start, None, date(2026, 10, 6).at(0, 0, 0, 0));
        assert_eq!(days(&got), [(9, 28), (9, 29), (9, 30), (10, 1), (10, 2), (10, 5)]);
        assert!(got.iter().all(|at| at.time() == start.time()));
    }

    #[test]
    fn every_other_week_counts_from_start() {
        let start = date(2026, 9, 2).at(15, 0, 0, 0);
        let got = run("FREQ=WEEKLY;INTERVAL=2;BYDAY=WE", start, None, date(2026, 10, 20).at(0, 0, 0, 0));
        assert_eq!(days(&got), [(9, 2), (9, 16), (9, 30), (10, 14)]);
    }

    #[test]
    fn count_and_until_both_stop_it() {
        let start = date(2026, 9, 1).at(8, 0, 0, 0);
        let far = date(2027, 1, 1).at(0, 0, 0, 0);
        assert_eq!(run("FREQ=DAILY;COUNT=3", start, None, far).len(), 3);
        let until = Some(date(2026, 9, 4).at(8, 0, 0, 0));
        assert_eq!(run("FREQ=DAILY", start, until, far).len(), 4);
    }

    #[test]
    fn monthly_by_weekday_and_last_day() {
        let far = date(2026, 5, 1).at(0, 0, 0, 0);
        let second_tuesdays = run("FREQ=MONTHLY;BYDAY=2TU", date(2026, 1, 13).at(10, 0, 0, 0), None, far);
        assert_eq!(days(&second_tuesdays), [(1, 13), (2, 10), (3, 10), (4, 14)]);
        let last_fridays = run("FREQ=MONTHLY;BYDAY=-1FR", date(2026, 1, 30).at(16, 0, 0, 0), None, far);
        assert_eq!(days(&last_fridays), [(1, 30), (2, 27), (3, 27), (4, 24)]);
        let month_ends = run("FREQ=MONTHLY;BYMONTHDAY=-1", date(2026, 1, 31).at(9, 0, 0, 0), None, far);
        assert_eq!(days(&month_ends), [(1, 31), (2, 28), (3, 31), (4, 30)]);
    }

    #[test]
    fn monthly_31st_skips_short_months() {
        let got = run("FREQ=MONTHLY", date(2026, 1, 31).at(9, 0, 0, 0), None, date(2026, 6, 1).at(0, 0, 0, 0));
        assert_eq!(days(&got), [(1, 31), (3, 31), (5, 31)]);
    }

    #[test]
    fn yearly_rules_like_dst_onsets() {
        let start = date(2007, 3, 11).at(2, 0, 0, 0);
        let got = run("FREQ=YEARLY;BYMONTH=3;BYDAY=2SU", start, None, date(2027, 1, 1).at(0, 0, 0, 0));
        assert_eq!(got.last().map(|at| at.date()), Some(date(2026, 3, 8)));
        let leap = run("FREQ=YEARLY", date(2020, 2, 29).at(0, 0, 0, 0), None, date(2029, 1, 1).at(0, 0, 0, 0));
        assert_eq!(leap.iter().map(|at| at.year()).collect::<Vec<_>>(), [2020, 2024, 2028]);
    }

    #[test]
    fn setpos_picks_last_weekday() {
        let start = date(2026, 1, 30).at(17, 0, 0, 0);
        let got = run("FREQ=MONTHLY;BYDAY=MO,TU,WE,TH,FR;BYSETPOS=-1", start, None, date(2026, 4, 1).at(0, 0, 0, 0));
        assert_eq!(days(&got), [(1, 30), (2, 27), (3, 31)]);
    }

    #[test]
    fn unknown_frequency_is_ignored() {
        assert!(Rule::parse("FREQ=HOURLY;INTERVAL=2").is_none());
        assert!(Rule::parse("INTERVAL=2").is_none());
        assert!(Rule::parse("FREQ=DAILY;INTERVAL=0").is_none());
    }
}
