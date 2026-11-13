//! The week calendar: a column per day with up to four event blocks, the last turning into +N when there are more.

use egui::{Align2, Color32, Id, Rect, Sense, pos2, vec2};
use notchbase_sdk::protocol::{MonthNode, UiValue, WeekDay, WeekEvent, WeekNode};

use crate::ui::color::{self, label, secondary_label, tertiary_label, white};
use crate::ui::{controls, font};

use super::style::ink;
use super::{Scene, liquid};

const HEADER_H: f32 = 48.0;
const WEEKDAY_Y: f32 = 12.0;
const NUMBER_Y: f32 = 32.0;
const NUMBER_R: f32 = 11.0;
const SLOTS: usize = 4;
const FEWEST: usize = 2;
const BLOCK_H: f32 = 46.0;
const BLOCK_GAP: f32 = 4.0;
const BOTTOM: f32 = 5.0;
const SIDE: f32 = 3.0;
const RADIUS: f32 = 7.0;
const PAD: f32 = 5.0;
const TOP: f32 = 4.0;
const TIME_H: f32 = 11.0;
const TINT: f32 = 0.3;
const TIME_GLOW: f32 = 0.35;
const SELECTED: f32 = 0.1;
const HOVER: f32 = 0.06;
const MORE: f32 = 0.06;

pub fn height(week: &WeekNode) -> f32 {
    let busiest = week.days.iter().map(|day| day.events.len()).max().unwrap_or(0);
    HEADER_H + busiest.clamp(FEWEST, SLOTS) as f32 * (BLOCK_H + BLOCK_GAP) + BOTTOM
}

pub(super) fn paint(scene: &mut Scene, week: &WeekNode, rect: Rect, id: Id) {
    super::swipe::turn(scene, week.on_scroll, rect, id);
    let key = week.days.first().map_or(0, |day| MonthNode::pack(day.year, day.month, day.day) as i64);
    let ctx = scene.ui.ctx().clone();
    let swap = super::turn::advance(&ctx, id.with("turn"), week, key);
    let live = scene.live;
    super::turn::slide(scene, rect, week, &swap, |scene, shown, current| match current {
        true => days(scene, shown, rect, id.with(("page", key)), live),
        false => days(scene, shown, rect, id.with("leaving"), false),
    });
}

fn days(scene: &mut Scene, week: &WeekNode, rect: Rect, id: Id, live: bool) {
    let width = rect.width() / week.days.len().max(1) as f32;
    let columns: Vec<Rect> = (0..week.days.len())
        .map(|index| {
            Rect::from_min_size(pos2(rect.left() + index as f32 * width, rect.top()), vec2(width, rect.height()))
        })
        .collect();
    if let Some(chosen) = week.days.iter().position(|day| day.selected) {
        let center = columns[chosen].center();
        let (trail, lead) = match live {
            true => liquid::follow(&scene.ui.ctx().clone(), id.with("selection"), center),
            false => (center, center),
        };
        let fill = color::over(white(SELECTED), Color32::BLACK);
        liquid::slab(scene.canvas, columns[chosen], trail.x, lead.x, fill);
    }
    for (index, (day, column)) in week.days.iter().zip(&columns).enumerate() {
        let column_id = id.with(("day", index));
        let pickable = live && week.on_day.is_some();
        let response = scene.ui.interact(*column, column_id, if pickable { Sense::click() } else { Sense::hover() });
        let hover = controls::animate(scene.ui, column_id.with("hover"), pickable && response.hovered());
        if hover > 0.0 && !day.selected {
            scene.canvas.rect(*column, 12.0, white(HOVER * hover));
        }
        date(scene, day, *column);
        blocks(scene, week, day, *column, column_id, live);
        if pickable && response.clicked() {
            scene.emit(week.on_day, UiValue::Index(MonthNode::pack(day.year, day.month, day.day)));
        }
    }
}

fn date(scene: &mut Scene, day: &WeekDay, column: Rect) {
    let x = column.center().x;
    let weekday = pos2(x, column.top() + WEEKDAY_Y);
    let ink = if day.today { color::RED } else { tertiary_label() };
    scene.canvas.text(weekday, Align2::CENTER_CENTER, &day.weekday, font::caption(), ink);
    let number = pos2(x, column.top() + NUMBER_Y);
    let ink = if day.today {
        scene.canvas.circle(number, NUMBER_R, color::RED);
        Color32::WHITE
    } else {
        label()
    };
    let font = if day.today || day.selected { font::semibold(font::body().size) } else { font::body() };
    scene.canvas.text(number, Align2::CENTER_CENTER, day.day.to_string(), font, ink);
}

fn slot(column: Rect, index: usize) -> Rect {
    let top = column.top() + HEADER_H + index as f32 * (BLOCK_H + BLOCK_GAP);
    Rect::from_min_max(pos2(column.left() + SIDE, top), pos2(column.right() - SIDE, top + BLOCK_H))
}

fn blocks(scene: &mut Scene, week: &WeekNode, day: &WeekDay, column: Rect, id: Id, live: bool) {
    let crowded = day.events.len() > SLOTS;
    let shown = if crowded { SLOTS - 1 } else { day.events.len() };
    for (index, event) in day.events.iter().take(shown).enumerate() {
        block(scene, week, event, slot(column, index), id.with(("event", index)), live);
    }
    if crowded {
        more(scene, day.events.len() - shown, slot(column, shown));
    }
}

fn block(scene: &mut Scene, week: &WeekNode, event: &WeekEvent, rect: Rect, id: Id, live: bool) {
    let pressable = live && week.on_event.is_some();
    let response = scene.ui.interact(rect, id, if pressable { Sense::click() } else { Sense::hover() });
    let hover = controls::animate(scene.ui, id.with("hover"), pressable && response.hovered());
    let tone = event.color.map_or(color::BLUE, ink);
    let fill =
        color::mix(color::over(tone.gamma_multiply(TINT), Color32::BLACK), tone.gamma_multiply(0.45), hover * 0.4);
    scene.canvas.rect(rect, RADIUS, fill);
    let inner = rect.width() - 2.0 * PAD;
    let mut top = rect.top() + TOP;
    if !event.time.is_empty() {
        let time_font = font::semibold(9.0);
        let time = controls::truncate(scene.canvas, &event.time, time_font.clone(), inner);
        let ink = color::mix(tone, Color32::WHITE, TIME_GLOW);
        scene.canvas.text(pos2(rect.left() + PAD, top), Align2::LEFT_TOP, time, time_font, ink);
        top += TIME_H;
    }
    let unbroken = event.title.split_whitespace().all(|word| scene.canvas.text_width(word, font::caption()) <= inner);
    let rows = match (unbroken, event.time.is_empty()) {
        (false, _) => 1,
        (true, true) => 3,
        (true, false) => 2,
    };
    let title = scene.canvas.clamped(event.title.clone(), font::caption(), label(), inner, rows);
    scene.canvas.add(egui::Shape::galley(pos2(rect.left() + PAD, top), title, label()));
    if pressable && response.clicked() {
        scene.emit(week.on_event, UiValue::Text(event.id.clone()));
    }
}

fn more(scene: &mut Scene, count: usize, rect: Rect) {
    let pill = Rect::from_min_size(rect.min, vec2(rect.width(), BLOCK_H * 0.5));
    scene.canvas.rect(pill, RADIUS, white(MORE));
    scene.canvas.text(pill.center(), Align2::CENTER_CENTER, format!("+{count}"), font::caption(), secondary_label());
}

#[cfg(test)]
mod tests {
    use notchbase_sdk::protocol::WeekEvent;

    use super::*;

    fn day(events: usize) -> WeekDay {
        WeekDay { events: vec![WeekEvent::default(); events], ..WeekDay::default() }
    }

    fn week(busiest: &[usize]) -> WeekNode {
        WeekNode { days: busiest.iter().map(|count| day(*count)).collect(), ..WeekNode::default() }
    }

    #[test]
    fn quiet_week_keeps_the_fewest_slots() {
        let fewest = HEADER_H + FEWEST as f32 * (BLOCK_H + BLOCK_GAP) + BOTTOM;
        assert_eq!(height(&week(&[])), fewest);
        assert_eq!(height(&week(&[0, 1, 0])), fewest);
    }

    #[test]
    fn busiest_day_sets_the_height() {
        assert_eq!(height(&week(&[1, 3, 2])), HEADER_H + 3.0 * (BLOCK_H + BLOCK_GAP) + BOTTOM);
        assert_eq!(height(&week(&[9])), height(&week(&[SLOTS])));
    }

    #[test]
    fn slots_stack_under_the_header() {
        let column = Rect::from_min_max(pos2(10.0, 100.0), pos2(60.0, 400.0));
        let first = slot(column, 0);
        assert_eq!(first, Rect::from_min_max(pos2(13.0, 148.0), pos2(57.0, 148.0 + BLOCK_H)));
        assert_eq!(slot(column, 1).top() - first.bottom(), BLOCK_GAP);
        assert_eq!(slot(column, 3).width(), first.width());
    }
}
