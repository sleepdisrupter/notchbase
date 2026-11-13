//! The picker node: drum wheels you drag, fling or scroll. They tick as each row passes and can loop around.

use std::collections::HashMap;

use egui::{Align2, Id, Rect, Sense, pos2, vec2};
use notchbase_sdk::protocol::{HandlerId, PickerColumn, UiValue};

use super::Scene;
use crate::ui::color::{self, label, tertiary_label};
use crate::ui::font;
use crate::ui::glass::{self, Glass};
use crate::ui::spring::{Spring, SpringConfig, UNIT_PRECISION, lerp};

pub(super) const ROW: f32 = 26.0;
pub(super) const ROWS: f32 = 5.0;
pub(super) const COLUMN_W: f32 = 68.0;
const STEP: f32 = 0.36;
const HIDDEN: f32 = 1.42;
const REACH: i64 = 3;
const MOMENTUM: f32 = 0.32;
const GRIP: f32 = 0.35;
const RUBBER: f32 = 0.3;
const FLING: SpringConfig = SpringConfig::new(0.5, 1.0);
const NUDGE: SpringConfig = SpringConfig::new(0.3, 0.8);
const FADE_TICK: SpringConfig = SpringConfig::new(0.22, 1.0);
const NUMBER: f32 = 21.0;
const SHRUNK: f32 = 0.7;
const NUMBER_AT: f32 = 0.42;
const UNIT_GAP: f32 = 5.0;
const BAND_PAD: f32 = 4.0;
const BAND_INSET: f32 = 6.0;

#[derive(Default)]
pub struct Drums {
    drums: HashMap<Id, Drum>,
}

impl Drums {
    pub fn step(&mut self, dt: f32) {
        for drum in self.drums.values_mut() {
            drum.at.step(dt);
            drum.tick.step(dt);
        }
    }

    pub fn is_animating(&self) -> bool {
        self.drums.values().any(|drum| drum.held || !drum.at.is_settled() || !drum.tick.is_settled())
    }
}

struct Drum {
    at: Spring,
    tick: Spring,
    held: bool,
    speed: f32,
    whole: i64,
    wanted: Option<usize>,
    wheel: crate::ui::scroll::Notches,
}

impl Drum {
    fn new(selected: usize) -> Self {
        let at = selected as f32;
        Self {
            at: Spring::new(at, FLING, UNIT_PRECISION),
            tick: Spring::new(0.0, FADE_TICK, UNIT_PRECISION),
            held: false,
            speed: 0.0,
            whole: at as i64,
            wanted: None,
            wheel: crate::ui::scroll::Notches::default(),
        }
    }

    fn index_of(&self, row: f32, column: &PickerColumn) -> usize {
        let count = column.options.len() as i64;
        let row = row.round() as i64;
        if column.looping { row.rem_euclid(count) as usize } else { row.clamp(0, count - 1) as usize }
    }

    fn land(&mut self, row: f32, column: &PickerColumn, config: SpringConfig) -> usize {
        let last = column.options.len().saturating_sub(1) as f32;
        let row = if column.looping { row.round() } else { row.round().clamp(0.0, last) };
        self.at.animate_to(row, config);
        self.index_of(row, column)
    }

    /// Turns to the tool's selection, unless we picked a row the tool hasn't caught up with yet.
    fn follow(&mut self, column: &PickerColumn) {
        if self.wanted.is_some_and(|wanted| wanted != column.selected) {
            return;
        }
        self.wanted = None;
        if self.held || self.index_of(self.at.target, column) == column.selected {
            return;
        }
        let count = column.options.len() as f32;
        let mut row = column.selected as f32;
        if column.looping {
            let base = (self.at.target / count).floor() * count;
            row = [base - count, base, base + count]
                .map(|start| start + column.selected as f32)
                .into_iter()
                .min_by(|a, b| (a - self.at.target).abs().total_cmp(&(b - self.at.target).abs()))
                .unwrap_or(row);
        }
        self.at.animate_to(row, NUDGE);
    }

    fn click_through(&mut self) -> bool {
        let whole = self.at.value.round() as i64;
        if whole == self.whole {
            return false;
        }
        self.whole = whole;
        self.tick.snap_to(1.0);
        self.tick.animate_to(0.0, FADE_TICK);
        true
    }
}

pub(super) fn paint(scene: &mut Scene, rect: Rect, id: Id, columns: &[PickerColumn], on_change: Option<HandlerId>) {
    let live = scene.live && on_change.is_some();
    let width = rect.width() / columns.len().max(1) as f32;
    let tick = columns
        .iter()
        .enumerate()
        .filter_map(|(c, _)| scene.state.drums.drums.get(&id.with(c)).map(|drum| drum.tick.value))
        .fold(0.0, f32::max);
    let band = Rect::from_center_size(rect.center(), vec2(rect.width() - 2.0 * BAND_INSET, ROW + BAND_PAD));
    glass::paint(scene.canvas, band, band.height() * 0.5, Glass::lerp(Glass::CONTROL, Glass::TILE_HOVER, tick * 0.8));
    for (c, column) in columns.iter().enumerate().filter(|(_, column)| !column.options.is_empty()) {
        let lane = Rect::from_min_size(pos2(rect.left() + c as f32 * width, rect.top()), vec2(width, rect.height()));
        if let Some(index) = drum(scene, lane, id.with(c), column, live) {
            scene.emit(on_change, UiValue::Picked { column: c, index });
        }
    }
}

fn drum(scene: &mut Scene, lane: Rect, id: Id, column: &PickerColumn, live: bool) -> Option<usize> {
    let response = scene.ui.interact(lane, id, if live { Sense::click_and_drag() } else { Sense::hover() });
    let dt = scene.ui.input(|input| input.stable_dt).clamp(1.0 / 240.0, 1.0 / 20.0);
    let notches = if live && response.hovered() { crate::ui::scroll::down(scene.ui) } else { 0.0 };
    let drum = scene.state.drums.drums.entry(id).or_insert_with(|| Drum::new(column.selected));
    let mut picked = None;
    drum.follow(column);
    if live {
        if response.drag_started() || (response.is_pointer_button_down_on() && !drum.held) {
            drum.held = true;
            drum.speed = 0.0;
            drum.at.snap_to(drum.at.value);
        }
        if response.dragged() {
            let last = column.options.len().saturating_sub(1) as f32;
            let mut rows = -response.drag_delta().y / ROW;
            // Dragged past either end, a plain wheel moves at a third of the speed, like rubber.
            if !column.looping && (drum.at.value < 0.0 || drum.at.value > last) {
                rows *= RUBBER;
            }
            drum.at.snap_to(drum.at.value + rows);
            drum.speed = lerp(drum.speed, rows / dt, GRIP);
        }
        if response.drag_stopped() {
            drum.held = false;
            let glide = drum.at.value + drum.speed * MOMENTUM;
            drum.at.velocity = drum.speed;
            picked = Some(drum.land(glide, column, FLING));
        } else if response.clicked() {
            drum.held = false;
            let offset = response.interact_pointer_pos().map_or(0.0, |at| ((at.y - lane.center().y) / ROW).round());
            picked = Some(drum.land(drum.at.value.round() + offset, column, NUDGE));
        } else if drum.held && !response.is_pointer_button_down_on() {
            drum.held = false;
            picked = Some(drum.land(drum.at.value, column, NUDGE));
        }
        let rows = drum.wheel.whole(notches);
        if rows != 0 {
            picked = Some(drum.land(drum.at.target - rows as f32, column, NUDGE));
        }
    }
    if drum.click_through() {
        crate::platform::haptic(crate::platform::Feel::Tick);
    }
    let at = drum.at.value;
    let picked = picked.filter(|index| *index != column.selected);
    drum.wanted = picked.or(drum.wanted);
    paint_rows(scene, lane, column, at);
    picked
}

fn paint_rows(scene: &mut Scene, lane: Rect, column: &PickerColumn, at: f32) {
    let count = column.options.len() as i64;
    let number_right = lane.left() + lane.width() * NUMBER_AT;
    let nearest = at.round() as i64;
    for row in nearest - REACH..=nearest + REACH {
        let angle = (row as f32 - at) * STEP;
        if angle.abs() >= HIDDEN {
            continue;
        }
        let index = if column.looping { row.rem_euclid(count) } else { row };
        let Some(option) = usize::try_from(index).ok().and_then(|index| column.options.get(index)) else { continue };
        let depth = angle.cos();
        let y = lane.center().y + angle.sin() / STEP * ROW;
        let focus = (1.0 - (row as f32 - at).abs()).clamp(0.0, 1.0);
        let tint = color::mix(tertiary_label(), label(), focus).gamma_multiply(depth * depth);
        let size = NUMBER * lerp(SHRUNK, 1.0, depth);
        scene.canvas.text(pos2(number_right, y), Align2::RIGHT_CENTER, option.as_str(), font::regular(size), tint);
    }
    if !column.unit.is_empty() {
        let at = pos2(number_right + UNIT_GAP, lane.center().y + 1.0);
        scene.canvas.text(at, Align2::LEFT_CENTER, column.unit.as_str(), font::semibold(12.0), label());
    }
}

pub(super) fn height() -> f32 {
    ROW * ROWS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minutes() -> PickerColumn {
        PickerColumn {
            options: (0..60).map(|m| m.to_string()).collect(),
            selected: 5,
            unit: "min".into(),
            looping: true,
        }
    }

    #[test]
    fn fling_lands_on_a_whole_row() {
        let column = minutes();
        let mut drum = Drum::new(5);
        drum.speed = 20.0;
        let glide = drum.at.value + drum.speed * MOMENTUM;
        let index = drum.land(glide, &column, FLING);
        assert_eq!(index, 11);
        assert_eq!(drum.at.target, 11.0);
    }

    #[test]
    fn looping_wheel_takes_short_way() {
        let column = PickerColumn { selected: 58, ..minutes() };
        let mut drum = Drum::new(1);
        assert_eq!(drum.index_of(-2.0, &column), 58);
        drum.follow(&column);
        assert_eq!(drum.at.target, -2.0, "back two rows, not forward fifty seven");
    }

    #[test]
    fn plain_wheel_stops_at_its_ends() {
        let column = PickerColumn { looping: false, ..minutes() };
        let mut drum = Drum::new(0);
        assert_eq!(drum.land(-4.0, &column, FLING), 0);
        assert_eq!(drum.land(99.0, &column, FLING), 59);
    }

    #[test]
    fn passing_a_row_clicks_the_band() {
        let mut drum = Drum::new(0);
        drum.at.snap_to(0.7);
        assert!(drum.click_through(), "each row that rolls past is one tick");
        assert_eq!(drum.tick.value, 1.0);
        assert!(!drum.click_through(), "staying on a row is silent");
        assert_eq!(drum.whole, 1);
    }

    #[test]
    fn pick_waits_for_the_tool() {
        let column = minutes();
        let mut drum = Drum::new(5);
        drum.wanted = Some(drum.land(9.0, &column, FLING));
        drum.follow(&column);
        assert_eq!(drum.at.target, 9.0, "the tool still says 5, but it asked for 9");
        drum.follow(&PickerColumn { selected: 9, ..column });
        assert_eq!((drum.at.target, drum.wanted), (9.0, None));
    }
}
