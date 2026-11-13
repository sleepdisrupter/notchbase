//! A soft light edge for dark colours, so an ink swatch or dot doesn't vanish into the dark pane.

use egui::epaint::RectShape;
use egui::{Color32, CornerRadius, Pos2, Rect, Vec2};

use crate::ui::canvas::Canvas;
use crate::ui::color::white;

const SINKS_BELOW: f32 = 0.2;
const REACH: f32 = 0.75;
const SOFTNESS: f32 = 2.5;
const GLOW: f32 = 0.36;
const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];

pub(super) fn paint(canvas: &mut Canvas, center: Pos2, radius: f32, color: Color32) {
    let sunk = sunk(color);
    if sunk <= 0.0 {
        return;
    }
    let disc = Rect::from_center_size(center, Vec2::splat((radius + REACH) * 2.0));
    let glow = RectShape::filled(disc, CornerRadius::same(u8::MAX), white(GLOW * sunk));
    canvas.add(glow.with_blur_width(SOFTNESS));
}

fn sunk(color: Color32) -> f32 {
    let channels = [color.r(), color.g(), color.b()].map(|channel| f32::from(channel) / f32::from(u8::MAX));
    let brightness: f32 = channels.iter().zip(LUMA).map(|(channel, weight)| channel * weight).sum();
    ((SINKS_BELOW - brightness) / SINKS_BELOW).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotate::Brush;
    use crate::ui::color;

    #[test]
    fn only_dark_colours_get_an_edge() {
        assert!(sunk(color::INK) > 0.5);
        assert_eq!(sunk(Color32::BLACK), 1.0);
        assert!(sunk(Color32::from_rgb(16, 32, 96)) > 0.0, "a dark blue from the config file");
        for bright in Brush::PALETTE.into_iter().filter(|swatch| *swatch != color::INK) {
            assert_eq!(sunk(bright), 0.0, "{bright:?}");
        }
    }
}
