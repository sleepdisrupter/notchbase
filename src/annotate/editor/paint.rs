//! Paints a frame: the picture, its size and copy status, the line or words in progress, the toolbar and brush ring.

use egui::{Align2, Pos2, Vec2, pos2};

use super::Editor;
use super::layout::Layout;
use super::pen;
use super::status::Reading;
use crate::annotate::Fit;
use crate::ui::canvas::Canvas;
use crate::ui::{color, font};

const WASH: f32 = 0.78;
const STATUS_INSET: f32 = 18.0;

impl Editor {
    pub(super) fn paint(&self, ui: &egui::Ui, layout: &Layout, fit: Fit, pointer: Option<Pos2>) {
        let painter = ui.painter();
        let body = if self.frosted { color::INK.gamma_multiply(WASH) } else { color::INK };
        painter.rect_filled(layout.window, 0.0, body);
        let mut canvas = Canvas::new(painter);
        self.picture.paint(&mut canvas, fit.rect(), layout.stage);
        let line = layout.title_line(self.title_bar);
        let [width, height] = self.sheet.size();
        let size = format!("{width} × {height}");
        let middle = pos2(layout.title.center().x, line);
        canvas.text(middle, Align2::CENTER_CENTER, size, font::callout(), color::secondary_label());
        let edge = pos2(layout.title.right() - STATUS_INSET, line);
        self.status.paint(&mut canvas, edge, self.reading());
        canvas.paint(painter, Pos2::ZERO, 1.0, Vec2::ZERO, 1.0);
        if let Some(pen) = &self.pen {
            pen.paint(painter, fit);
        }
        if let Some(typing) = &self.typing {
            typing.paint(painter, fit);
        }
        self.toolbar.paint(painter, layout, self.brush);
        let presence = self.ring.value.clamp(0.0, 1.0);
        if let Some(at) = pointer.filter(|_| presence > 0.0) {
            pen::paint_ring(painter, at, self.brush, presence);
        }
    }

    pub(super) fn reading(&self) -> Option<Reading> {
        if self.is_writing() { None } else { self.status.reading(self.sheet.revision()) }
    }
}
