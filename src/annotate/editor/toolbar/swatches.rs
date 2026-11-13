//! The five colour swatches, with a ring that slides to the chosen one.

use egui::{Id, Rect, Sense, Stroke, pos2};

use crate::annotate::Brush;
use crate::ui::canvas::Canvas;
use crate::ui::spring::{Spring, UNIT_PRECISION, lerp};
use crate::ui::{color, motion};

use super::super::layout::SWATCHES;
use super::super::touch::Touch;
use super::rim;

const BEAD: f32 = 9.0;
const GROW: f32 = 1.5;
const RING: f32 = 12.5;
const RING_WIDTH: f32 = 2.0;
const RING_INK: f32 = 0.92;

pub(super) struct Swatches {
    touches: [Touch; SWATCHES],
    ring: Spring,
    marked: Spring,
}

impl Swatches {
    pub(super) fn new(brush: Brush) -> Self {
        let chosen = chosen(brush);
        Self {
            touches: Default::default(),
            ring: Spring::new(chosen.unwrap_or(0) as f32, motion::BOUNCY, UNIT_PRECISION),
            marked: Spring::new(if chosen.is_some() { 1.0 } else { 0.0 }, motion::HOVER, UNIT_PRECISION),
        }
    }

    pub(super) fn step(&mut self, brush: Brush, dt: f32) {
        let chosen = chosen(brush);
        if let Some(index) = chosen {
            self.ring.animate_to(index as f32, motion::BOUNCY);
        }
        self.marked.animate_to(if chosen.is_some() { 1.0 } else { 0.0 }, motion::HOVER);
        self.ring.step(dt);
        self.marked.step(dt);
        self.touches.iter_mut().for_each(|touch| touch.step(dt));
    }

    pub(super) fn is_settled(&self) -> bool {
        self.ring.is_settled() && self.marked.is_settled() && self.touches.iter().all(Touch::is_settled)
    }

    pub(super) fn sense(&mut self, ui: &egui::Ui, slots: &[Rect; SWATCHES], id: Id) -> Option<usize> {
        let mut picked = None;
        for (index, (slot, touch)) in slots.iter().zip(&mut self.touches).enumerate() {
            let response = ui.interact(*slot, id.with(index), Sense::CLICK);
            touch.follow(&response, true);
            if response.clicked() {
                picked = Some(index);
            }
        }
        picked
    }

    pub(super) fn paint(&self, canvas: &mut Canvas, slots: &[Rect; SWATCHES]) {
        let (first, last) = (slots[0].center(), slots[SWATCHES - 1].center());
        let along = self.ring.value / (SWATCHES - 1) as f32;
        let ring = pos2(lerp(first.x, last.x, along), first.y);
        let ink = color::white(RING_INK * self.marked.value.clamp(0.0, 1.0));
        canvas.ring(ring, RING, Stroke::new(RING_WIDTH, ink));
        for ((slot, touch), color) in slots.iter().zip(&self.touches).zip(Brush::PALETTE) {
            let radius = (BEAD + GROW * touch.hover()) * touch.squeeze();
            rim::paint(canvas, slot.center(), radius, color);
            canvas.circle(slot.center(), radius, color);
        }
    }
}

fn chosen(brush: Brush) -> Option<usize> {
    Brush::PALETTE.iter().position(|color| *color == brush.color())
}
