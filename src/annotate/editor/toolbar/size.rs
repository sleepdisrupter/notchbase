//! The size slider and its well, which shows the brush as a dot or a letter. Thin sizes get more of the track.

use egui::epaint::{CornerRadius, Shadow};
use egui::{Color32, Id, Rect, Sense, Vec2, pos2, vec2};

use super::super::command::Command;
use super::super::tool::Tool;
use super::super::touch::Touch;
use super::rim;
use crate::annotate::Brush;
use crate::ui::canvas::{self, Canvas};
use crate::ui::color::white;
use crate::ui::glyph::{self, Glyph};
use crate::ui::motion;
use crate::ui::spring::{Spring, UNIT_PRECISION, lerp};

const TRACK: f32 = 4.0;
const KNOB: f32 = 7.0;
const KNOB_HELD: f32 = 8.5;
const FINE_END: f32 = 2.0;
const STEP: f32 = 0.5;
const GROOVE: f32 = 0.14;
const FILL: f32 = 0.88;
const KNOB_SHADOW: Shadow = Shadow { offset: [0, 1], blur: 6, spread: 0, color: Color32::from_black_alpha(110) };
const LETTER_REACH: f32 = 0.5;

pub(super) struct SizeControl {
    touch: Touch,
    sliding: bool,
    lettered: Spring,
}

impl Default for SizeControl {
    fn default() -> Self {
        Self { touch: Touch::default(), sliding: false, lettered: Spring::new(0.0, motion::HOVER, UNIT_PRECISION) }
    }
}

impl SizeControl {
    pub(super) fn step(&mut self, tool: Tool, dt: f32) {
        self.touch.step(dt);
        self.lettered.animate_to(if tool == Tool::Text { 1.0 } else { 0.0 }, motion::HOVER);
        self.lettered.step(dt);
    }

    pub(super) fn is_settled(&self) -> bool {
        self.touch.is_settled() && self.lettered.is_settled()
    }

    pub(super) fn sense(&mut self, ui: &egui::Ui, area: Rect, id: Id) -> Option<Command> {
        let response = ui.interact(area, id, Sense::CLICK | Sense::DRAG);
        self.touch.follow(&response, true);
        let held = response.is_pointer_button_down_on();
        let released = self.sliding && !held;
        self.sliding = held;
        if released {
            return Some(Command::Settle);
        }
        let at = response.interact_pointer_pos().filter(|_| held)?;
        Some(Command::Resize(size_at(at.x, area)))
    }

    pub(super) fn paint(&self, canvas: &mut Canvas, well: Rect, area: Rect, brush: Brush) {
        let lettered = self.lettered.value.clamp(0.0, 1.0);
        dot(canvas, well, brush, 1.0 - lettered);
        letter(canvas, well, brush, lettered);
        let (from, to) = travel(area);
        let knob = pos2(lerp(from, to, share(brush.size())), area.center().y);
        let groove = Rect::from_center_size(area.center(), vec2(area.width(), TRACK));
        canvas.rect(groove, TRACK / 2.0, white(GROOVE));
        canvas.rect(Rect::from_min_max(groove.min, pos2(knob.x, groove.bottom())), TRACK / 2.0, white(FILL));
        let radius = lerp(KNOB, KNOB_HELD, self.touch.hover());
        let disc = Rect::from_center_size(knob, Vec2::splat(radius * 2.0));
        canvas.add(KNOB_SHADOW.as_shape(disc, CornerRadius::same(radius.round() as u8)));
        canvas.circle(knob, radius, white(1.0));
    }
}

fn dot(canvas: &mut Canvas, well: Rect, brush: Brush, presence: f32) {
    if presence <= 0.0 {
        return;
    }
    let mark = canvas.mark();
    rim::paint(canvas, well.center(), brush.size() / 2.0, brush.color());
    canvas.circle(well.center(), brush.size() / 2.0, brush.color());
    canvas.since(mark).iter_mut().for_each(|shape| canvas::fade(shape, presence));
}

fn letter(canvas: &mut Canvas, well: Rect, brush: Brush, presence: f32) {
    if presence <= 0.0 {
        return;
    }
    let mark = canvas.mark();
    let tall = brush.letter_size().min(well.height());
    rim::paint(canvas, well.center(), tall * LETTER_REACH, brush.color());
    glyph::paint(canvas, Glyph::Text, well.center(), tall, brush.color());
    canvas.since(mark).iter_mut().for_each(|shape| canvas::fade(shape, presence));
}

fn travel(area: Rect) -> (f32, f32) {
    (area.left() + KNOB_HELD, area.right() - KNOB_HELD)
}

fn share(size: f32) -> f32 {
    let span = Brush::THICKEST - Brush::THINNEST;
    ((size - Brush::THINNEST) / span).clamp(0.0, 1.0).powf(FINE_END.recip())
}

fn size_at(x: f32, area: Rect) -> f32 {
    let (from, to) = travel(area);
    let along = if to > from { ((x - from) / (to - from)).clamp(0.0, 1.0) } else { 0.0 };
    let size = lerp(Brush::THINNEST, Brush::THICKEST, along.powf(FINE_END));
    (size / STEP).round() * STEP
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: Rect = Rect { min: pos2(100.0, 10.0), max: pos2(204.0, 42.0) };

    #[test]
    fn track_ends_at_the_limits() {
        assert_eq!(size_at(0.0, AREA), Brush::THINNEST);
        assert_eq!(size_at(100.0 + KNOB_HELD, AREA), Brush::THINNEST);
        assert_eq!(size_at(204.0 - KNOB_HELD, AREA), Brush::THICKEST);
        assert_eq!(size_at(900.0, AREA), Brush::THICKEST);
    }

    #[test]
    fn sizes_land_on_half_points() {
        for x in 100..=204 {
            let size = size_at(x as f32, AREA);
            assert_eq!(size * 2.0, (size * 2.0).round(), "{x}");
        }
    }

    #[test]
    fn thin_sizes_get_more_track() {
        let (from, to) = travel(AREA);
        let middle = size_at((from + to) / 2.0, AREA);
        assert!(middle < (Brush::THINNEST + Brush::THICKEST) / 2.0, "{middle}");
        assert!(share(Brush::DEFAULT_SIZE) > 0.25 && share(Brush::DEFAULT_SIZE) < 0.4);
    }

    #[test]
    fn knob_sits_where_size_was_picked() {
        let (from, to) = travel(AREA);
        for size in [2.0, 3.5, 5.0, 12.0, 20.5, 32.0] {
            let x = lerp(from, to, share(size));
            assert_eq!(size_at(x, AREA), size);
        }
    }
}
