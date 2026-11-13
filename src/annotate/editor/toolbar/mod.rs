//! The glass toolbar: tools, colour swatches, the size control, undo, redo and Done. It rises into place on opening.

mod buttons;
mod rim;
mod size;
mod swatches;
#[cfg(test)]
mod tests;
mod tools;

use egui::epaint::RectShape;
use egui::{Color32, CornerRadius, Id, Painter, Pos2, Stroke, pos2, vec2};

use self::buttons::Button;
use self::size::SizeControl;
use self::swatches::Swatches;
use self::tools::Tools;
use super::command::Command;
use super::layout::Layout;
use super::tool::Tool;
use crate::annotate::Brush;
use crate::ui::canvas::Canvas;
use crate::ui::color::white;
use crate::ui::glass::{self, Glass};
use crate::ui::glyph::Glyph;
use crate::ui::spring::{Spring, SpringConfig, UNIT_PRECISION};

const RISE: SpringConfig = SpringConfig::new(0.55, 0.84);
const RISES_FROM: f32 = 18.0;
const PANE_CLEAR: f32 = 0.72;
const PANE: Glass =
    Glass { tint: Color32::from_rgb(40, 40, 43), sheen: 0.05, specular: 0.16, edge: 0.0, clear: PANE_CLEAR };
const SHADE: u8 = 70;
const SHADE_DROP: f32 = 6.0;
const SHADE_BLUR: f32 = 24.0;
const DIVIDER: f32 = 0.09;

pub(super) struct Available {
    pub(super) undo: bool,
    pub(super) redo: bool,
}

pub(super) struct Toolbar {
    rise: Spring,
    tools: Tools,
    swatches: Swatches,
    size: SizeControl,
    undo: Button,
    redo: Button,
    done: Button,
}

impl Toolbar {
    pub(super) fn new(brush: Brush, tool: Tool) -> Self {
        let mut rise = Spring::new(0.0, RISE, UNIT_PRECISION);
        rise.animate_to(1.0, RISE);
        Self {
            rise,
            tools: Tools::new(tool),
            swatches: Swatches::new(brush),
            size: SizeControl::default(),
            undo: Button::default(),
            redo: Button::default(),
            done: Button::default(),
        }
    }

    pub(super) fn step(&mut self, brush: Brush, tool: Tool, dt: f32) {
        self.rise.step(dt);
        self.tools.step(tool, dt);
        self.swatches.step(brush, dt);
        self.size.step(tool, dt);
        [&mut self.undo, &mut self.redo, &mut self.done].into_iter().for_each(|button| button.step(dt));
    }

    pub(super) fn is_settled(&self) -> bool {
        self.rise.is_settled()
            && self.tools.is_settled()
            && self.swatches.is_settled()
            && self.size.is_settled()
            && [&self.undo, &self.redo, &self.done].into_iter().all(Button::is_settled)
    }

    pub(super) fn sense(&mut self, ui: &egui::Ui, layout: &Layout, id: Id, available: &Available) -> Vec<Command> {
        let used = self.tools.sense(ui, &layout.tools, id.with("tool")).map(Command::Use);
        let picked = self.swatches.sense(ui, &layout.swatches, id.with("swatch")).map(Command::Pick);
        let sized = self.size.sense(ui, layout.slider, id.with("size"));
        let undone = self.undo.sense(ui, layout.undo, id.with("undo"), available.undo).then_some(Command::Undo);
        let redone = self.redo.sense(ui, layout.redo, id.with("redo"), available.redo).then_some(Command::Redo);
        let done = self.done.sense(ui, layout.done, id.with("done"), true).then_some(Command::Close);
        [used, picked, sized, undone, redone, done].into_iter().flatten().collect()
    }

    pub(super) fn paint(&self, painter: &Painter, layout: &Layout, brush: Brush) {
        let mut canvas = Canvas::new(painter);
        let radius = layout.toolbar.height() / 2.0;
        let shade = RectShape::filled(
            layout.toolbar.translate(vec2(0.0, SHADE_DROP)),
            CornerRadius::same(radius.round() as u8),
            Color32::from_black_alpha(SHADE),
        );
        canvas.add(shade.with_blur_width(SHADE_BLUR));
        glass::paint(&mut canvas, layout.toolbar, radius, PANE);
        let [top, bottom] = layout.divider_span();
        for x in layout.dividers {
            canvas.add(egui::Shape::line_segment([pos2(x, top), pos2(x, bottom)], Stroke::new(1.0, white(DIVIDER))));
        }
        self.tools.paint(&mut canvas, &layout.tools);
        self.swatches.paint(&mut canvas, &layout.swatches);
        self.size.paint(&mut canvas, layout.well, layout.slider, brush);
        self.undo.paint_round(&mut canvas, layout.undo, Glyph::Undo);
        self.redo.paint_round(&mut canvas, layout.redo, Glyph::Redo);
        self.done.paint_filled(&mut canvas, layout.done);
        let risen = self.rise.value;
        let below = vec2(0.0, (1.0 - risen) * RISES_FROM);
        canvas.paint(painter, Pos2::ZERO, 1.0, below, risen.clamp(0.0, 1.0));
    }
}
