//! The brush and text switch, with a thumb that slides to the chosen tool.

use egui::epaint::{CornerRadius, Shadow};
use egui::{Color32, Id, Rect, Sense, Vec2, pos2};

use crate::ui::canvas::{Canvas, scale_about};
use crate::ui::spring::{Spring, UNIT_PRECISION, lerp};
use crate::ui::{color, glyph, motion};

use super::super::layout::TOOLS;
use super::super::tool::Tool;
use super::super::touch::Touch;

const GLYPH: f32 = 15.0;
const GROOVE: f32 = 0.08;
const THUMB_INSET: f32 = 2.0;
const THUMB_SHADOW: Shadow = Shadow { offset: [0, 1], blur: 6, spread: 0, color: Color32::from_black_alpha(110) };
const RESTING: f32 = 0.6;

pub(super) struct Tools {
    touches: [Touch; TOOLS],
    thumb: Spring,
}

impl Tools {
    pub(super) fn new(tool: Tool) -> Self {
        Self { touches: Default::default(), thumb: Spring::new(tool.index() as f32, motion::BOUNCY, UNIT_PRECISION) }
    }

    pub(super) fn step(&mut self, tool: Tool, dt: f32) {
        self.thumb.animate_to(tool.index() as f32, motion::BOUNCY);
        self.thumb.step(dt);
        self.touches.iter_mut().for_each(|touch| touch.step(dt));
    }

    pub(super) fn is_settled(&self) -> bool {
        self.thumb.is_settled() && self.touches.iter().all(Touch::is_settled)
    }

    pub(super) fn sense(&mut self, ui: &egui::Ui, slots: &[Rect; TOOLS], id: Id) -> Option<Tool> {
        let mut picked = None;
        for ((slot, touch), tool) in slots.iter().zip(&mut self.touches).zip(Tool::ALL) {
            let response = ui.interact(*slot, id.with(tool.index()), Sense::CLICK);
            touch.follow(&response, true);
            if response.clicked() {
                picked = Some(tool);
            }
        }
        picked
    }

    pub(super) fn paint(&self, canvas: &mut Canvas, slots: &[Rect; TOOLS]) {
        let (first, last) = (slots[0], slots[TOOLS - 1]);
        let groove = first.union(last);
        canvas.rect(groove, groove.height() / 2.0, color::white(GROOVE));
        let along = self.thumb.value / (TOOLS - 1) as f32;
        let middle = pos2(lerp(first.center().x, last.center().x, along), first.center().y);
        let thumb = Rect::from_center_size(middle, first.size() - Vec2::splat(2.0 * THUMB_INSET));
        let radius = thumb.height() / 2.0;
        canvas.add(THUMB_SHADOW.as_shape(thumb, CornerRadius::same(radius.round() as u8)));
        canvas.rect(thumb, radius, color::PEARL);
        for ((slot, touch), tool) in slots.iter().zip(&self.touches).zip(Tool::ALL) {
            let mark = canvas.mark();
            let chosen = (1.0 - (self.thumb.value - tool.index() as f32).abs()).clamp(0.0, 1.0);
            let resting = color::white(lerp(RESTING, 1.0, touch.hover()));
            let tint = color::mix(resting, color::ink_on(color::PEARL), chosen);
            glyph::paint(canvas, tool.glyph(), slot.center(), GLYPH, tint);
            let squeeze = scale_about(slot.center(), touch.squeeze());
            canvas.since(mark).iter_mut().for_each(|shape| shape.transform(squeeze));
        }
    }
}
