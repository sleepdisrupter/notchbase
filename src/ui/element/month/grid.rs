//! Lays a month out over six weeks of cells, filling the ends with days from the months on either side.

use egui::{Pos2, Rect, pos2, vec2};

pub(super) const HEADER_H: f32 = 20.0;
pub(super) const ROW_H: f32 = 34.0;
pub(super) const ROWS: usize = 6;
pub(super) const LIFT: f32 = 3.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Cell {
    pub(super) day: u8,
    pub(super) inside: bool,
    pub(super) year: i32,
    pub(super) month: u8,
}

pub(super) struct Grid {
    rect: Rect,
    width: f32,
}

impl Grid {
    pub(super) fn new(rect: Rect) -> Grid {
        Grid { rect, width: rect.width() / 7.0 }
    }

    pub(super) fn header(&self, column: usize) -> Pos2 {
        pos2(self.rect.left() + (column as f32 + 0.5) * self.width, self.rect.top() + HEADER_H * 0.5)
    }

    pub(super) fn slot(&self, index: usize) -> Rect {
        let (row, column) = (index / 7, index % 7);
        Rect::from_min_size(
            pos2(self.rect.left() + column as f32 * self.width, self.rect.top() + HEADER_H + row as f32 * ROW_H),
            vec2(self.width, ROW_H),
        )
    }

    pub(super) fn center(&self, index: usize) -> Pos2 {
        self.slot(index).center() - vec2(0.0, LIFT)
    }
}

pub(super) fn cells(year: i32, month: u8, monday_first: bool) -> Vec<Cell> {
    let lead = usize::from((weekday(year, month, 1) + if monday_first { 0 } else { 1 }) % 7);
    let length = days_in(year, month);
    let (before_year, before_month) = if month == 1 { (year - 1, 12) } else { (year, month - 1) };
    let before = days_in(before_year, before_month);
    let (after_year, after_month) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
    (0..ROWS * 7)
        .map(|index| match index.checked_sub(lead).map(|offset| offset as u8 + 1) {
            None => {
                Cell { day: before - (lead - index) as u8 + 1, inside: false, year: before_year, month: before_month }
            }
            Some(day) if day <= length => Cell { day, inside: true, year, month },
            Some(day) => Cell { day: day - length, inside: false, year: after_year, month: after_month },
        })
        .collect()
}

fn days_in(year: i32, month: u8) -> u8 {
    match month {
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn weekday(year: i32, month: u8, day: u8) -> u8 {
    // Sakamoto's method. It counts from Sunday, so the result is shifted to start on Monday.
    const SHIFT: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let year = if month < 3 { year - 1 } else { year };
    let sunday_zero =
        (year + year / 4 - year / 100 + year / 400 + SHIFT[usize::from(month - 1)] + i32::from(day)).rem_euclid(7);
    ((sunday_zero + 6) % 7) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inside(cells: &[Cell]) -> Vec<u8> {
        cells.iter().filter(|cell| cell.inside).map(|cell| cell.day).collect()
    }

    #[test]
    fn weekdays_count_from_monday() {
        assert_eq!(weekday(2026, 9, 30), 2);
        assert_eq!(weekday(2026, 3, 1), 6);
        assert_eq!(weekday(2024, 2, 29), 3);
    }

    #[test]
    fn month_lines_up_under_weekdays() {
        let monday = cells(2026, 9, true);
        assert_eq!(
            monday[0],
            Cell { day: 31, inside: false, year: 2026, month: 8 },
            "September 2026 starts on a Tuesday"
        );
        assert_eq!(monday[1], Cell { day: 1, inside: true, year: 2026, month: 9 });
        let sunday = cells(2026, 9, false);
        assert_eq!(sunday[2].day, 1);
        assert_eq!(inside(&monday), (1..=30).collect::<Vec<u8>>());
        assert_eq!(monday.len(), 42);
        assert_eq!(monday[41], Cell { day: 11, inside: false, year: 2026, month: 10 });
    }

    #[test]
    fn february_and_first_column_starts() {
        assert_eq!(inside(&cells(2024, 2, true)).len(), 29);
        assert_eq!(inside(&cells(2026, 2, true)).len(), 28);
        let june = cells(2026, 6, true);
        assert_eq!(june[0], Cell { day: 1, inside: true, year: 2026, month: 6 }, "June 2026 starts on a Monday");
        let january = cells(2026, 1, false);
        assert_eq!(january[0], Cell { day: 28, inside: false, year: 2025, month: 12 });
        let december = cells(2026, 12, true);
        assert_eq!((december[41].year, december[41].month), (2027, 1), "the last row runs into the new year");
    }

    #[test]
    fn cells_sit_in_seven_columns() {
        let grid = Grid::new(Rect::from_min_size(pos2(0.0, 0.0), vec2(350.0, 224.0)));
        assert_eq!(grid.slot(0).min, pos2(0.0, HEADER_H));
        assert_eq!(grid.slot(8).min, pos2(50.0, HEADER_H + ROW_H));
        assert_eq!(grid.center(0), pos2(25.0, HEADER_H + ROW_H * 0.5 - LIFT));
    }
}
