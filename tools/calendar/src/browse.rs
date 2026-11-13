//! Moving through the days and months on the calendar page.

use jiff::civil::Date;
use jiff::tz::TimeZone;
use notchbase_sdk::element::Month;
use notchbase_sdk::prelude::*;

use crate::day::{midnight, shift};
use crate::meeting::Meeting;

const CELLS: i64 = 42;
pub const DAYS: i64 = 4;
const MARGIN: i64 = 7;
const MOST_MARKS: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Browse {
    pub first: Date,
    pub selected: Date,
}

impl Browse {
    pub fn at(today: Date) -> Browse {
        Browse { first: today.first_of_month(), selected: today }
    }

    pub fn shift(&mut self, months: i32, today: Date) {
        let total = i32::from(self.first.year()) * 12 + i32::from(self.first.month() - 1) + months;
        let (Ok(year), month) = (i16::try_from(total.div_euclid(12)), (total.rem_euclid(12) + 1) as i8) else { return };
        let Ok(first) = Date::new(year, month, 1) else { return };
        self.first = first;
        self.selected = if today.first_of_month() == first { today } else { first };
    }

    pub fn pick(&mut self, year: i32, month: u8, day: u8) {
        let picked = i16::try_from(year).ok().and_then(|year| Date::new(year, month as i8, day as i8).ok());
        if let Some(picked) = picked {
            self.selected = picked;
            self.first = picked.first_of_month();
        }
    }

    pub fn shift_days(&mut self, days: i64) {
        self.selected = shift(self.selected, days);
        self.first = self.selected.first_of_month();
    }

    pub fn days(&self, today: Date) -> Vec<Date> {
        let gap = self.selected.since(today).map_or(0, |span| i64::from(span.get_days()));
        let start = shift(today, gap.div_euclid(DAYS) * DAYS);
        (0..DAYS).map(|index| shift(start, index)).collect()
    }

    pub fn days_title(&self, today: Date) -> String {
        let days = self.days(today);
        crate::day::week_title(days[0], days[days.len() - 1])
    }

    pub fn follow(&mut self, before: Date, today: Date) {
        if self.selected == before {
            *self = Browse::at(today);
        }
    }

    pub fn title(&self) -> String {
        crate::day::month_title(self.first)
    }

    pub fn range(&self, monday_first: bool, zone: &TimeZone) -> (u64, u64) {
        let grid = self.grid_start(monday_first);
        let start = grid.min(shift(self.selected, -MARGIN));
        let end = shift(grid, CELLS).max(shift(self.selected, MARGIN + 1));
        (midnight(start, zone), midnight(end, zone))
    }

    pub fn selected_range(&self, zone: &TimeZone) -> (u64, u64) {
        (midnight(self.selected, zone), midnight(shift(self.selected, 1), zone))
    }

    pub fn grid<Msg>(&self, today: Date, meetings: &[Meeting], monday_first: bool, zone: &TimeZone) -> Month<Msg> {
        let mut grid = month(i32::from(self.first.year()), self.first.month() as u8).monday_first(monday_first);
        if today.first_of_month() == self.first {
            grid = grid.today(today.day() as u8);
        }
        if self.selected.first_of_month() == self.first {
            grid = grid.selected(self.selected.day() as u8);
        }
        for day in 1..=self.first.days_in_month() {
            let date = Date::new(self.first.year(), self.first.month(), day).unwrap_or(self.first);
            let (from, to) = (midnight(date, zone), midnight(shift(date, 1), zone));
            let mut colors: Vec<Color> = Vec::new();
            for meeting in meetings.iter().filter(|meeting| !meeting.declined && meeting.overlaps(from, to)) {
                let color = meeting.color.unwrap_or_else(|| Tone::Blue.into());
                if !colors.contains(&color) && colors.len() < MOST_MARKS {
                    colors.push(color);
                }
            }
            for color in colors {
                grid = grid.mark(day as u8, color);
            }
        }
        grid
    }

    fn grid_start(&self, monday_first: bool) -> Date {
        let offset = self.first.weekday().to_monday_zero_offset() + if monday_first { 0 } else { 1 };
        shift(self.first, -i64::from(offset % 7))
    }
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;

    use super::*;

    #[test]
    fn month_moves_land_on_first_or_today() {
        let today = date(2026, 9, 30);
        let mut browse = Browse::at(today);
        browse.shift(1, today);
        assert_eq!((browse.first, browse.selected), (date(2026, 10, 1), date(2026, 10, 1)));
        browse.shift(-13, today);
        assert_eq!(browse.first, date(2025, 9, 1));
        browse.shift(12, today);
        assert_eq!(browse.selected, today, "back in this month, today is picked again");
        assert_eq!(browse.title(), "September 2026");
    }

    #[test]
    fn days_page_a_few_at_a_time() {
        let today = date(2026, 9, 30);
        let mut browse = Browse::at(today);
        assert_eq!(browse.days(today), [date(2026, 9, 30), date(2026, 10, 1), date(2026, 10, 2), date(2026, 10, 3)]);
        assert_eq!(browse.days_title(today), "Sep 30 \u{2013} Oct 3");
        browse.shift_days(DAYS);
        assert_eq!((browse.selected, browse.first), (date(2026, 10, 4), date(2026, 10, 1)));
        assert_eq!(browse.days(today)[0], date(2026, 10, 4));
        browse.pick(2026, 9, 29);
        assert_eq!(browse.days(today)[0], date(2026, 9, 26), "earlier days page back from today too");
    }

    #[test]
    fn range_covers_the_selection() {
        let mut browse = Browse::at(date(2026, 9, 30));
        browse.pick(2026, 6, 1);
        let (from, to) = browse.range(true, &TimeZone::UTC);
        assert!(
            from <= midnight(date(2026, 5, 25), &TimeZone::UTC) && to >= midnight(date(2026, 6, 9), &TimeZone::UTC)
        );
    }

    #[test]
    fn missing_day_changes_nothing() {
        let mut browse = Browse::at(date(2026, 2, 10));
        browse.pick(2026, 2, 14);
        assert_eq!(browse.selected, date(2026, 2, 14));
        browse.pick(2026, 2, 30);
        assert_eq!(browse.selected, date(2026, 2, 14));
    }

    #[test]
    fn grid_covers_six_weeks() {
        let zone = TimeZone::UTC;
        let browse = Browse::at(date(2026, 9, 30));
        let (from, to) = browse.range(true, &zone);
        assert_eq!(from, midnight(date(2026, 8, 31), &zone));
        assert_eq!(to - from, 42 * 24 * 3_600_000);
        assert_eq!(browse.range(false, &zone).0, midnight(date(2026, 8, 30), &zone));
    }

    #[test]
    fn rolls_over_only_from_today() {
        let mut browse = Browse::at(date(2026, 9, 30));
        browse.follow(date(2026, 9, 30), date(2026, 10, 1));
        assert_eq!(browse, Browse::at(date(2026, 10, 1)));
        browse.pick(2026, 10, 12);
        browse.follow(date(2026, 10, 1), date(2026, 10, 2));
        assert_eq!(browse.selected, date(2026, 10, 12));
    }
}
