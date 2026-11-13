//! Pill buttons, text links and rows of buttons lined up on the right.

use egui::{Align2, Color32, Id, Rect, Response, Sense, pos2, vec2};

use crate::canvas::Canvas;
use crate::color::{self, label, secondary_label};
use crate::glass::{self, Glass};
use crate::{controls, font};

const PRESS_INSET: f32 = 1.0;
const TEXT_ROOM: f32 = 16.0;
const DISABLED: f32 = 0.4;

#[derive(Clone, Copy, PartialEq)]
pub enum Tone {
    Primary,
    Plain,
}

pub struct Button<'a> {
    pub rect: Rect,
    pub id: Id,
    pub title: &'a str,
    pub tone: Tone,
    pub enabled: bool,
}

/// Returns true when clicked. With `live` off the button is drawn but takes no input.
pub fn button(ui: &egui::Ui, canvas: &mut Canvas, button: &Button<'_>, live: bool) -> bool {
    let Button { rect, id, title, tone, enabled } = *button;
    let response = (live && enabled).then(|| ui.interact(rect, id, Sense::click()));
    let hover = controls::animate(ui, id.with("hover"), response.as_ref().is_some_and(Response::hovered));
    let down = response.as_ref().is_some_and(Response::is_pointer_button_down_on);
    let drawn = if down { rect.shrink(PRESS_INSET) } else { rect };
    let radius = rect.height() * 0.5;
    let fade = if enabled { 1.0 } else { DISABLED };
    match tone {
        Tone::Primary => {
            canvas.rect(drawn, radius, color::mix(color::ACCENT, Color32::WHITE, 0.12 * hover).gamma_multiply(fade));
        }
        Tone::Plain => glass::paint(canvas, drawn, radius, Glass::lerp(Glass::CONTROL, Glass::TILE_HOVER, hover)),
    }
    let text = controls::truncate(canvas, title, font::headline(), rect.width() - TEXT_ROOM);
    let tint = if tone == Tone::Primary { color::ink_on(color::ACCENT) } else { label() };
    canvas.text(rect.center(), Align2::CENTER_CENTER, text, font::headline(), tint.gamma_multiply(fade));
    response.is_some_and(|r| r.clicked())
}

pub fn width(canvas: &Canvas, title: &str, least: f32) -> f32 {
    (canvas.text_width(title, font::headline()) + 2.0 * TEXT_ROOM).max(least)
}

pub fn link(ui: &egui::Ui, canvas: &mut Canvas, row: Rect, id: Id, title: &str, right: f32, live: bool) -> bool {
    let room = right - row.left();
    if room <= 0.0 {
        return false;
    }
    let text = controls::truncate(canvas, title, font::callout(), room);
    let width = canvas.text_width(text.as_str(), font::callout());
    let rect = Rect::from_min_size(pos2(row.left(), row.top()), vec2(width, row.height()));
    let response = live.then(|| ui.interact(rect, id, Sense::click()));
    let hover = controls::animate(ui, id.with("hover"), response.as_ref().is_some_and(Response::hovered));
    let tint = color::mix(secondary_label(), label(), hover);
    canvas.text(pos2(rect.left(), rect.center().y), Align2::LEFT_CENTER, text, font::callout(), tint);
    response.is_some_and(|r| r.clicked())
}

/// Lays out buttons of the given widths from the right edge of `row`, keeping their order.
pub fn trailing(row: Rect, widths: &[f32], gap: f32) -> Vec<Rect> {
    let mut right = row.right();
    let mut rects: Vec<Rect> = widths
        .iter()
        .rev()
        .map(|width| {
            let rect = Rect::from_min_max(pos2(right - width, row.top()), pos2(right, row.bottom()));
            right = rect.left() - gap;
            rect
        })
        .collect();
    rects.reverse();
    rects
}

#[cfg(test)]
mod tests {
    use egui::{LayerId, Painter, Shape};

    use super::*;

    fn context() -> egui::Context {
        let ctx = egui::Context::default();
        font::install(&ctx);
        ctx
    }

    fn painted(tone: Tone, enabled: bool, live: bool) -> (bool, Vec<Shape>) {
        let ctx = context();
        let mut result = (false, Vec::new());
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let mut canvas = Canvas::new(ui.painter());
            let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(120.0, 30.0));
            let pressed =
                button(ui, &mut canvas, &Button { rect, id: Id::new("b"), title: "Install", tone, enabled }, live);
            result = (pressed, canvas.shapes);
        });
        output.textures_delta.clear();
        result
    }

    #[test]
    fn trailing_lines_up_from_the_right() {
        let row = Rect::from_min_max(pos2(0.0, 100.0), pos2(300.0, 130.0));
        let rects = trailing(row, &[80.0, 60.0], 8.0);
        assert_eq!(rects.len(), 2);
        assert_eq!((rects[1].left(), rects[1].right()), (240.0, 300.0));
        assert_eq!((rects[0].left(), rects[0].right()), (152.0, 232.0));
        assert!(rects.iter().all(|rect| rect.y_range() == row.y_range()));
        assert!(trailing(row, &[], 8.0).is_empty());
    }

    #[test]
    fn width_grows_with_title() {
        let ctx = context();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let canvas = Canvas::new(ui.painter());
            assert_eq!(width(&canvas, "", 76.0), 76.0);
            let long = "Install notchbase on this computer now";
            let text = canvas.text_width(long, font::headline());
            assert_eq!(width(&canvas, long, 76.0), text + 2.0 * TEXT_ROOM);
        });
        output.textures_delta.clear();
    }

    #[test]
    fn primary_is_pill_plain_is_glass() {
        let (pressed, primary) = painted(Tone::Primary, true, true);
        assert!(!pressed);
        assert!(matches!(primary[..], [Shape::Rect(_), Shape::Text(_)]), "{primary:?}");
        let (_, plain) = painted(Tone::Plain, true, true);
        assert!(plain.len() > 2 && matches!(plain.last(), Some(Shape::Text(_))));
    }

    #[test]
    fn disabled_fades_and_ignores_clicks() {
        let (pressed, faded) = painted(Tone::Primary, false, true);
        let (_, bright) = painted(Tone::Primary, true, true);
        assert!(!pressed);
        let fill = |shapes: &[Shape]| match &shapes[0] {
            Shape::Rect(rect) => rect.fill.a(),
            _ => panic!("a pill"),
        };
        assert!(fill(&faded) < fill(&bright));
    }

    #[test]
    fn link_without_room_is_skipped() {
        let ctx = context();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let mut canvas = Canvas::new(&Painter::new(ctx.clone(), LayerId::background(), Rect::EVERYTHING));
            let row = Rect::from_min_size(pos2(50.0, 0.0), vec2(200.0, 20.0));
            assert!(!link(ui, &mut canvas, row, Id::new("l"), "Elsewhere", 40.0, true));
            assert!(canvas.shapes.is_empty());
            assert!(!link(ui, &mut canvas, row, Id::new("l"), "Elsewhere", 250.0, false));
            assert!(matches!(canvas.shapes[..], [Shape::Text(_)]));
        });
        output.textures_delta.clear();
    }
}
