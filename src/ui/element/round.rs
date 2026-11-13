//! Round buttons with a ring and a glass face that squish in when pressed and spring back when let go.

use std::collections::HashMap;

use egui::{Align2, Color32, Id, Rect, Sense, Stroke, Vec2};
use notchbase_sdk::protocol::Tone;

use crate::ui::canvas::fade;
use crate::ui::color::{self, label};
use crate::ui::glass::{self, Glass};
use crate::ui::spring::{Spring, SpringConfig, UNIT_PRECISION};
use crate::ui::{controls, font};

use super::Scene;
use super::style::tone;

const PRESS_IN: SpringConfig = SpringConfig::new(0.12, 1.0);
const LET_GO: SpringConfig = SpringConfig::new(0.42, 0.42);
const SQUISH: f32 = 0.09;
const RING_GAP: f32 = 3.5;
const RING_W: f32 = 1.6;
const RING_ALPHA: f32 = 0.55;
const LABEL_SCALE: f32 = 0.22;
const HOVER_LIFT: f32 = 0.2;
const DISABLED: f32 = 0.4;

#[derive(Default)]
pub struct Presses {
    springs: HashMap<Id, Spring>,
}

impl Presses {
    pub fn step(&mut self, dt: f32) {
        for spring in self.springs.values_mut() {
            spring.step(dt);
        }
    }

    pub fn is_animating(&self) -> bool {
        self.springs.values().any(|spring| !spring.is_settled())
    }

    fn press(&mut self, id: Id, down: bool) -> f32 {
        let spring = self.springs.entry(id).or_insert_with(|| Spring::new(0.0, LET_GO, UNIT_PRECISION));
        let target = if down { 1.0 } else { 0.0 };
        if spring.target != target {
            spring.animate_to(target, if down { PRESS_IN } else { LET_GO });
        }
        spring.value
    }
}

pub(super) struct Round<'a> {
    pub(super) label: &'a str,
    pub(super) tone: Tone,
    pub(super) size: f32,
    pub(super) enabled: bool,
}

pub(super) fn paint(scene: &mut Scene, rect: Rect, id: Id, button: &Round) -> bool {
    let live = scene.live && button.enabled;
    let circle = Rect::from_center_size(rect.center(), Vec2::splat(button.size));
    let response = scene.ui.interact(circle, id, if live { Sense::click() } else { Sense::hover() });
    let hover = controls::animate(scene.ui, id.with("hover"), live && response.hovered());
    let press = scene.state.presses.press(id, live && response.is_pointer_button_down_on());
    let scale = 1.0 - SQUISH * press;
    let (fill, ink) = colors(button.tone);
    let radius = button.size * 0.5 * scale;
    let mark = scene.canvas.mark();
    let ring = Stroke::new(RING_W, fill.gamma_multiply(RING_ALPHA + HOVER_LIFT * hover));
    scene.canvas.ring(circle.center(), radius - RING_W * 0.5, ring);
    let inner = Rect::from_center_size(circle.center(), Vec2::splat(2.0 * (radius - RING_GAP)));
    let resting = Glass::tinted_by(fill);
    let lit = Glass::tinted_by(color::mix(fill, Color32::WHITE, 0.25));
    glass::paint(
        scene.canvas,
        inner,
        inner.height() * 0.5,
        Glass::lerp(resting, lit, hover * HOVER_LIFT + press * 0.3),
    );
    let text = font::semibold(button.size * LABEL_SCALE * scale);
    scene.canvas.text(circle.center(), Align2::CENTER_CENTER, button.label, text, ink);
    if !button.enabled {
        for shape in scene.canvas.since(mark) {
            fade(shape, DISABLED);
        }
    }
    live && response.clicked()
}

fn colors(t: Tone) -> (Color32, Color32) {
    match t {
        Tone::Gray | Tone::Primary | Tone::Secondary | Tone::Tertiary | Tone::Quaternary | Tone::White => {
            (Color32::from_gray(120), label())
        }
        other => (tone(other), tone(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: f32 = 1.0 / 120.0;

    #[test]
    fn press_squishes_and_springs_back() {
        let mut presses = Presses::default();
        let id = Id::new("round");
        assert_eq!(presses.press(id, false), 0.0);
        assert!(!presses.is_animating());
        presses.press(id, true);
        assert!(presses.is_animating());
        (0..60).for_each(|_| presses.step(FRAME));
        assert!((presses.press(id, true) - 1.0).abs() < 0.01, "held down it settles pressed");
        presses.press(id, false);
        (0..240).for_each(|_| presses.step(FRAME));
        assert!(presses.press(id, false).abs() < 0.01, "let go it settles back");
        assert!(!presses.is_animating());
    }

    #[test]
    fn each_round_button_has_its_own_press() {
        let mut presses = Presses::default();
        let (held, idle) = (Id::new("held"), Id::new("idle"));
        presses.press(held, true);
        presses.press(idle, false);
        (0..30).for_each(|_| presses.step(FRAME));
        assert!(presses.press(held, true) > 0.5);
        assert_eq!(presses.press(idle, false), 0.0);
    }

    #[test]
    fn neutral_tones_are_gray() {
        for neutral in [Tone::Gray, Tone::Primary, Tone::Secondary, Tone::Tertiary, Tone::Quaternary, Tone::White] {
            assert_eq!(colors(neutral), (Color32::from_gray(120), label()), "{neutral:?}");
        }
        for colored in [Tone::Red, Tone::Green, Tone::Blue] {
            assert_eq!(colors(colored), (tone(colored), tone(colored)), "{colored:?}");
        }
    }
}
