//! Undo, redo and Done. The round ones dim while there is nothing to step to.

use egui::{Align2, Color32, Id, Rect, Sense};

use crate::ui::canvas::{self, Canvas, scale_about};
use crate::ui::glass::{self, Glass};
use crate::ui::glyph::{self, Glyph};
use crate::ui::spring::{Spring, UNIT_PRECISION, lerp};
use crate::ui::{color, font, motion};

use super::super::touch::Touch;

const GLYPH: f32 = 15.0;
const RESTING: f32 = 0.3;
const DONE: &str = "Done";
const DONE_LIFT: f32 = 0.7;
const RESTING_GLASS: Glass =
    Glass { tint: Color32::from_rgb(58, 58, 62), sheen: 0.05, specular: 0.2, edge: 0.14, clear: glass::CLEAR };
const HOVERED_GLASS: Glass =
    Glass { tint: Color32::from_rgb(74, 74, 79), sheen: 0.07, specular: 0.3, edge: 0.14, clear: glass::CLEAR };

pub(super) struct Button {
    touch: Touch,
    live: Spring,
}

impl Default for Button {
    fn default() -> Self {
        Self { touch: Touch::default(), live: Spring::new(0.0, motion::HOVER, UNIT_PRECISION) }
    }
}

impl Button {
    pub(super) fn step(&mut self, dt: f32) {
        self.touch.step(dt);
        self.live.step(dt);
    }

    pub(super) fn is_settled(&self) -> bool {
        self.touch.is_settled() && self.live.is_settled()
    }

    pub(super) fn sense(&mut self, ui: &egui::Ui, rect: Rect, id: Id, live: bool) -> bool {
        let response = ui.interact(rect, id, if live { Sense::CLICK } else { Sense::HOVER });
        self.touch.follow(&response, live);
        self.live.animate_to(if live { 1.0 } else { 0.0 }, motion::HOVER);
        live && response.clicked()
    }

    pub(super) fn paint_round(&self, canvas: &mut Canvas, rect: Rect, glyph: Glyph) {
        let mark = canvas.mark();
        let surface = Glass::lerp(RESTING_GLASS, HOVERED_GLASS, self.touch.hover());
        glass::paint(canvas, rect, rect.height() / 2.0, surface);
        glyph::paint(canvas, glyph, rect.center(), GLYPH, color::label());
        let (squeeze, alpha) = (scale_about(rect.center(), self.touch.squeeze()), self.liveliness());
        for shape in canvas.since(mark) {
            shape.transform(squeeze);
            canvas::fade(shape, alpha);
        }
    }

    pub(super) fn paint_filled(&self, canvas: &mut Canvas, rect: Rect) {
        let mark = canvas.mark();
        let fill = color::mix(color::PEARL, Color32::WHITE, self.touch.hover() * DONE_LIFT);
        canvas.rect(rect, rect.height() / 2.0, fill);
        canvas.text(rect.center(), Align2::CENTER_CENTER, DONE, font::headline(), color::ink_on(fill));
        let squeeze = scale_about(rect.center(), self.touch.squeeze());
        canvas.since(mark).iter_mut().for_each(|shape| shape.transform(squeeze));
    }

    fn liveliness(&self) -> f32 {
        lerp(RESTING, 1.0, self.live.value.clamp(0.0, 1.0))
    }
}
