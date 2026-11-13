//! The month calendar: six weeks under the weekday initials, today on a red disc, and up to three event dots a day.

mod grid;

use egui::{Align2, Color32, Id, Pos2, Rect, Sense};
use notchbase_sdk::protocol::{Color, MonthNode, UiValue};

use crate::ui::color::{self, label, quaternary_label, tertiary_label, white};
use crate::ui::{controls, font};

use super::style::ink;
use super::{Scene, liquid};
use grid::{Cell, Grid, HEADER_H, ROW_H, ROWS, cells};

pub const HEIGHT: f32 = HEADER_H + ROWS as f32 * ROW_H;
const RADIUS: f32 = 13.0;
const DOT: f32 = 1.8;
const DOT_GAP: f32 = 5.0;
const MOST_DOTS: usize = 3;
const HOVER: f32 = 0.1;
const CHOSEN: f32 = 0.2;
const MONDAY_FIRST: [&str; 7] = ["M", "T", "W", "T", "F", "S", "S"];
const SUNDAY_FIRST: [&str; 7] = ["S", "M", "T", "W", "T", "F", "S"];

pub(super) fn paint(scene: &mut Scene, month: &MonthNode, rect: Rect, id: Id) {
    super::swipe::turn(scene, month.on_scroll, rect, id);
    let grid = Grid::new(rect);
    let initials = if month.monday_first { MONDAY_FIRST } else { SUNDAY_FIRST };
    for (column, initial) in initials.iter().enumerate() {
        scene.canvas.text(grid.header(column), Align2::CENTER_CENTER, *initial, font::caption(), tertiary_label());
    }
    let ctx = scene.ui.ctx().clone();
    let swap = super::turn::advance(&ctx, id.with("turn"), month, i64::from(month.year) * 12 + i64::from(month.month));
    let live = scene.live && month.on_select.is_some();
    super::turn::slide(scene, rect, month, &swap, |scene, shown, current| match current {
        true => days(scene, &grid, shown, id, live),
        false => days(scene, &grid, shown, id.with("leaving"), false),
    });
}

fn days(scene: &mut Scene, grid: &Grid, month: &MonthNode, id: Id, live: bool) {
    let key = month.year * 12 + i32::from(month.month);
    let cells = cells(month.year, month.month, month.monday_first);
    let chosen = month.selected.and_then(|day| cells.iter().position(|cell| cell.inside && cell.day == day));
    if let Some(index) = chosen {
        let ctx = scene.ui.ctx().clone();
        let (trail, lead) = liquid::follow(&ctx, id.with(("selection", key)), grid.center(index));
        liquid::drop(scene.canvas, trail, lead, RADIUS, color::over(white(CHOSEN), Color32::BLACK));
    }
    for (index, cell) in cells.iter().enumerate() {
        let center = grid.center(index);
        let hover = if live { pointed(scene, grid, index, cell, id, month) } else { 0.0 };
        if !cell.inside {
            if hover > 0.0 {
                scene.canvas.circle(center, RADIUS, white(HOVER * hover));
            }
            let ink = color::mix(quaternary_label(), tertiary_label(), hover);
            scene.canvas.text(center, Align2::CENTER_CENTER, cell.day.to_string(), font::body(), ink);
            continue;
        }
        day(scene, center, cell.day, month.today == Some(cell.day), chosen == Some(index), hover);
        let colors = month.marks.iter().find(|mark| mark.day == cell.day).map(|mark| mark.colors.as_slice());
        dots(scene, center, colors.unwrap_or_default());
    }
}

fn pointed(scene: &mut Scene, grid: &Grid, index: usize, cell: &Cell, id: Id, month: &MonthNode) -> f32 {
    let cell_id = id.with(("day", cell.year, cell.month, cell.day));
    let response = scene.ui.interact(grid.slot(index).shrink(1.0), cell_id, Sense::click());
    let hover = controls::animate(scene.ui, cell_id.with("hover"), response.hovered());
    if response.clicked() {
        scene.emit(month.on_select, UiValue::Index(MonthNode::pack(cell.year, cell.month, cell.day)));
    }
    hover
}

fn day(scene: &mut Scene, center: Pos2, day: u8, today: bool, chosen: bool, hover: f32) {
    if today {
        scene.canvas.circle(center, RADIUS, color::RED);
    } else if hover > 0.0 && !chosen {
        scene.canvas.circle(center, RADIUS, white(HOVER * hover));
    }
    let (font, ink): (_, Color32) = match (today, chosen) {
        (true, _) => (font::semibold(font::body().size), Color32::WHITE),
        (false, true) => (font::semibold(font::body().size), label()),
        (false, false) => (font::body(), label()),
    };
    scene.canvas.text(center, Align2::CENTER_CENTER, day.to_string(), font, ink);
}

fn dots(scene: &mut Scene, center: Pos2, colors: &[Color]) {
    let shown = colors.len().min(MOST_DOTS);
    let start = center.x - (shown.saturating_sub(1)) as f32 * DOT_GAP * 0.5;
    for (index, color) in colors.iter().take(shown).enumerate() {
        let at = egui::pos2(start + index as f32 * DOT_GAP, center.y + RADIUS + DOT + 1.0);
        scene.canvas.circle(at, DOT, ink(*color));
    }
}

#[cfg(test)]
mod tests {
    use egui::pos2;

    use super::super::fixture::Stage;
    use super::*;

    const RED: Color = Color { r: 255, g: 0, b: 0, a: 255 };
    const GREEN: Color = Color { r: 0, g: 255, b: 0, a: 255 };
    const CENTER: Pos2 = pos2(100.0, 50.0);

    fn dotted(colors: &[Color]) -> Vec<Pos2> {
        let painted = Stage::new().with(|scene| dots(scene, CENTER, colors));
        painted.circles().iter().map(|circle| circle.center).collect()
    }

    fn october(monday_first: bool) -> MonthNode {
        MonthNode { year: 2026, month: 10, monday_first, ..MonthNode::default() }
    }

    #[test]
    fn month_is_header_and_six_weeks() {
        assert_eq!(HEIGHT, HEADER_H + 6.0 * ROW_H);
    }

    #[test]
    fn day_shows_at_most_three_dots() {
        let below = CENTER.y + RADIUS + DOT + 1.0;
        assert!(dotted(&[]).is_empty());
        assert_eq!(dotted(&[RED]), [pos2(100.0, below)]);
        assert_eq!(dotted(&[RED, GREEN]), [pos2(97.5, below), pos2(102.5, below)]);
        let crowded = dotted(&[RED, GREEN, RED, GREEN, RED]);
        assert_eq!(crowded, [pos2(95.0, below), pos2(100.0, below), pos2(105.0, below)]);
    }

    #[test]
    fn dots_take_their_marks_colours() {
        let painted = Stage::new().with(|scene| dots(scene, CENTER, &[RED, GREEN]));
        let fills: Vec<_> = painted.circles().iter().map(|circle| circle.fill).collect();
        assert_eq!(fills, [ink(RED), ink(GREEN)]);
    }

    #[test]
    fn today_sits_on_a_red_disc() {
        let painted = Stage::new().with(|scene| day(scene, CENTER, 10, true, false, 0.0));
        let disc = painted.circles()[0];
        assert_eq!((disc.center, disc.radius, disc.fill), (CENTER, RADIUS, color::RED));
        assert_eq!(painted.texts()[0].1, "10");
    }

    #[test]
    fn plain_day_is_just_its_number() {
        let still = Stage::new().with(|scene| day(scene, CENTER, 3, false, false, 0.0));
        assert!(still.circles().is_empty());
        let hovered = Stage::new().with(|scene| day(scene, CENTER, 3, false, false, 0.5));
        assert_eq!(hovered.circles()[0].fill, white(HOVER * 0.5));
        let chosen = Stage::new().with(|scene| day(scene, CENTER, 3, false, true, 0.5));
        assert!(chosen.circles().is_empty(), "the selection drop is painted under a chosen day");
    }

    #[test]
    fn initials_follow_first_weekday() {
        let rect = Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(280.0, HEIGHT));
        for (monday_first, initials) in [(true, MONDAY_FIRST), (false, SUNDAY_FIRST)] {
            let mut stage = Stage::new();
            let painted = stage.with(|scene| paint(scene, &october(monday_first), rect, Id::new("month")));
            let header: Vec<String> = painted.texts().into_iter().take(7).map(|(_, text)| text).collect();
            assert_eq!(header, initials.map(str::to_owned));
        }
    }

    #[test]
    fn every_day_is_painted() {
        let rect = Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(280.0, HEIGHT));
        let mut stage = Stage::new();
        let painted = stage.with(|scene| paint(scene, &october(true), rect, Id::new("month")));
        let numbers: Vec<String> = painted.texts().into_iter().skip(7).map(|(_, text)| text).collect();
        assert_eq!(numbers.len(), cells(2026, 10, true).len());
        for day in 1..=31 {
            assert!(numbers.contains(&day.to_string()), "{day}");
        }
    }
}
