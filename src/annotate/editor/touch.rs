//! Hover and press springs shared by the toolbar's controls.

use egui::Response;

use crate::ui::motion;
use crate::ui::spring::{Spring, UNIT_PRECISION};

const PRESSED: f32 = 0.92;

pub(super) struct Touch {
    hover: Spring,
    press: Spring,
}

impl Default for Touch {
    fn default() -> Self {
        Self {
            hover: Spring::new(0.0, motion::HOVER, UNIT_PRECISION),
            press: Spring::new(1.0, motion::PRESS, UNIT_PRECISION),
        }
    }
}

impl Touch {
    pub(super) fn follow(&mut self, response: &Response, live: bool) {
        let held = live && response.is_pointer_button_down_on();
        let over = live && (response.hovered() || held);
        self.hover.animate_to(if over { 1.0 } else { 0.0 }, motion::HOVER);
        self.press.animate_to(if held { PRESSED } else { 1.0 }, motion::PRESS);
    }

    pub(super) fn step(&mut self, dt: f32) {
        self.hover.step(dt);
        self.press.step(dt);
    }

    pub(super) fn is_settled(&self) -> bool {
        self.hover.is_settled() && self.press.is_settled()
    }

    pub(super) fn hover(&self) -> f32 {
        self.hover.value.clamp(0.0, 1.0)
    }

    pub(super) fn squeeze(&self) -> f32 {
        self.press.value
    }
}
