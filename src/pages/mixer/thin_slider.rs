//! The thin volume slider the mixer uses.

use egui::epaint::{CornerRadius, Shadow};
use egui::{Color32, Rect, Vec2, pos2, vec2};

use crate::ui::canvas::Canvas;
use crate::ui::color::white;
use crate::ui::glass::{self, Glass};
use crate::ui::meter::{self, Reading, Tint};
use crate::ui::spring::lerp;

pub(super) const KNOB_R: f32 = 6.0;
pub(super) const KNOB_R_DRAG: f32 = 7.0;
const TRACK_H: f32 = 6.0;
const KNOB_SHADOW: Shadow = Shadow { offset: [0, 1], blur: 6, spread: 0, color: Color32::from_black_alpha(90) };

pub(super) struct ThinSlider {
    track: Rect,
    x0: f32,
    x1: f32,
}

impl ThinSlider {
    pub(super) fn within(area: Rect) -> Self {
        let track = Rect::from_center_size(area.center(), vec2(area.width(), TRACK_H));
        Self { track, x0: track.left() + KNOB_R_DRAG, x1: track.right() - KNOB_R_DRAG }
    }

    pub(super) fn value_at(&self, x: f32) -> f32 {
        track_value(x, self.x0, self.x1)
    }

    pub(super) fn paint(&self, c: &mut Canvas, value: f32, reading: Reading, mute: f32, knob_r: f32) {
        let track = self.track;
        let radius = TRACK_H * 0.5;
        glass::paint(c, track, radius, Glass::CONTROL);
        let knob_x = lerp(self.x0, self.x1, value);
        let fill = Rect::from_min_max(track.min, pos2(knob_x.max(track.left() + TRACK_H), track.bottom()));
        c.rect(fill, radius, white(lerp(0.85, 0.28, mute)));
        if mute < 0.5 {
            let span = knob_x - track.left();
            meter::paint(c, track, span, reading, span, Tint::LEVEL);
        }
        let knob = Rect::from_center_size(pos2(knob_x, track.center().y), Vec2::splat(knob_r * 2.0));
        let corner = CornerRadius::same(knob_r.round().clamp(0.0, 255.0) as u8);
        c.add(KNOB_SHADOW.as_shape(knob, corner));
        c.circle(knob.center(), knob_r, white(lerp(1.0, 0.8, mute)));
    }
}

fn track_value(x: f32, x0: f32, x1: f32) -> f32 {
    if x1 <= x0 { 0.0 } else { ((x - x0) / (x1 - x0)).clamp(0.0, 1.0) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thin_slider_mapping() {
        assert_eq!(track_value(10.0, 10.0, 110.0), 0.0);
        assert_eq!(track_value(60.0, 10.0, 110.0), 0.5);
        assert_eq!(track_value(500.0, 10.0, 110.0), 1.0);
        assert_eq!(track_value(-5.0, 10.0, 110.0), 0.0);
        assert_eq!(track_value(5.0, 10.0, 10.0), 0.0);
    }

    #[test]
    fn knob_stays_inside_track() {
        let slider = ThinSlider::within(Rect::from_min_size(pos2(100.0, 0.0), vec2(114.0, 28.0)));
        assert_eq!(slider.value_at(100.0 + KNOB_R_DRAG), 0.0);
        assert_eq!(slider.value_at(214.0 - KNOB_R_DRAG), 1.0);
    }
}
