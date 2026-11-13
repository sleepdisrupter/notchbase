//! The on/off switch.

use egui::{Color32, Id, Rect, Sense, pos2, vec2};

use crate::canvas::Canvas;
use crate::color;
use crate::controls;
use crate::spring::smoothstep;

const SLIDE_SECONDS: f32 = 0.2;
const KNOB_INSET: f32 = 2.0;

pub fn switch(ui: &egui::Ui, canvas: &mut Canvas, rect: Rect, id: Id, on: bool, live: bool) -> bool {
    let response = live.then(|| ui.interact(rect, id, Sense::click()));
    let clicked = response.as_ref().is_some_and(|r| r.clicked());
    let hover = controls::animate(ui, id.with("hover"), response.as_ref().is_some_and(|r| r.hovered()));
    let slid = ui.ctx().animate_value_with_time(id.with("on"), if on { 1.0 } else { 0.0 }, SLIDE_SECONDS);
    let t = smoothstep(0.0, 1.0, slid);
    let track = color::mix(color::quaternary_label(), color::GREEN, t);
    canvas.rect(rect, rect.height() * 0.5, color::mix(track, Color32::WHITE, 0.06 * hover));
    let knob = rect.height() - 2.0 * KNOB_INSET;
    let x = rect.left() + KNOB_INSET + knob * 0.5 + (rect.width() - knob - 2.0 * KNOB_INSET) * t;
    let center = pos2(x, rect.center().y);
    canvas.circle(center + vec2(0.0, 0.6), knob * 0.5 + 0.4, Color32::from_black_alpha(60));
    canvas.circle(center, knob * 0.5, Color32::WHITE);
    clicked
}

#[cfg(test)]
mod tests {
    use egui::{Pos2, Shape, pos2};

    use super::*;

    const TRACK: Rect = Rect::from_min_max(pos2(10.0, 10.0), pos2(50.0, 34.0));

    fn drawn(on: bool, live: bool) -> (bool, Vec<Shape>) {
        let ctx = egui::Context::default();
        let mut result = (false, Vec::new());
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let mut canvas = Canvas::new(ui.painter());
            let clicked = switch(ui, &mut canvas, TRACK, Id::new("switch"), on, live);
            result = (clicked, canvas.shapes);
        });
        output.textures_delta.clear();
        result
    }

    fn knob(shapes: &[Shape]) -> (Pos2, f32) {
        let Shape::Circle(knob) = &shapes[2] else { panic!("a knob") };
        (knob.center, knob.radius)
    }

    #[test]
    fn knob_rests_on_its_side() {
        let (clicked, off) = drawn(false, true);
        assert!(!clicked, "nothing pressed it");
        assert_eq!(off.len(), 3, "the track, the knob's shadow and the knob");
        let knob_radius = (TRACK.height() - 2.0 * KNOB_INSET) * 0.5;
        assert_eq!(knob(&off), (pos2(TRACK.left() + KNOB_INSET + knob_radius, TRACK.center().y), knob_radius));
        let (_, on) = drawn(true, false);
        assert_eq!(knob(&on).0, pos2(TRACK.right() - KNOB_INSET - knob_radius, TRACK.center().y));
    }

    #[test]
    fn on_track_is_green() {
        let (_, on) = drawn(true, true);
        let (_, off) = drawn(false, true);
        let track = |shapes: &[Shape]| match &shapes[0] {
            Shape::Rect(rect) => rect.fill,
            _ => panic!("a track"),
        };
        assert_eq!(track(&on), color::GREEN);
        assert_eq!(track(&off), color::quaternary_label());
    }
}
