//! The thin grey rim around a floating island.

use egui::{Color32, Painter, Pos2, Shape};

use crate::platform::Style;
use crate::ui::shape;

use super::Notch;
use super::geometry::Silhouette;

pub(super) const WIDTH: f32 = 1.6;
pub(super) const SHOWN: f32 = 0.002;
const GRAY: u8 = 29;

pub(super) fn color(strength: f32) -> Color32 {
    Color32::from_gray(GRAY).gamma_multiply(strength.clamp(0.0, 1.0))
}

impl Notch {
    pub(super) fn keyline(&self, s: &Silhouette) -> f32 {
        match self.style {
            Style::Island => 1.0 - s.docking(),
            Style::Notch => 0.0,
        }
    }

    pub(super) fn paint_keyline(
        &self,
        painter: &Painter,
        outline: &[Pos2],
        s: &Silhouette,
        black_at: impl Fn(f32) -> f32,
    ) {
        let strength = self.keyline(s);
        if strength <= SHOWN {
            return;
        }
        let tint = |at: Pos2| color(strength * black_at(at.y));
        painter.add(Shape::mesh(shape::band_around(outline, WIDTH, tint, painter.pixels_per_point())));
    }
}

#[cfg(test)]
mod tests {
    use egui::{Rect, vec2};

    use super::*;
    use crate::notch::preview::Preview;
    use crate::notch::tests::fixtures::{Rig, island, shortcuts};
    use crate::notch::tests::snapshot::SCALE;

    const FRAMES: usize = 30;

    fn shot(style: Style, preview: Preview) -> (image::RgbImage, Rect) {
        let mut n = Notch::with_preview(style, Some(vec2(185.0, 32.0)), Some(preview));
        let image = Rig::new().shoot(&mut n, &shortcuts(None), FRAMES);
        (image, n.silhouette().rect())
    }

    fn pixel(image: &image::RgbImage, x: f32, y: f32) -> [u8; 3] {
        image.get_pixel((x * SCALE) as u32, (y * SCALE) as u32).0
    }

    fn is_keyline(seen: [u8; 3]) -> bool {
        seen.iter().all(|&channel| channel.abs_diff(GRAY) <= 2)
    }

    #[test]
    fn only_floating_island_has_keyline() {
        let mut n = island();
        assert_eq!(n.keyline(&n.silhouette()), 0.0, "the tab tucked into the top edge has none");
        let mut shown = 0.0;
        for step in 1..=100 {
            n.attract.snap_to(step as f32 / 100.0);
            let next = n.keyline(&n.silhouette());
            assert!(next >= shown && next - shown <= 0.021, "it comes in smoothly as the island lets go of the edge");
            shown = next;
        }
        assert_eq!(shown, 1.0, "and the floating capsule has all of it");
        let docked = Notch::with_preview(Style::Island, None, Some(Preview::Docked));
        assert_eq!(docked.keyline(&docked.silhouette()), 0.0, "a song docked against the edge has none");
        let notch = Notch::with_preview(Style::Notch, Some(vec2(185.0, 32.0)), Some(Preview::Peek));
        assert_eq!(notch.keyline(&notch.silhouette()), 0.0, "a hardware notch never has one");
    }

    #[test]
    fn island_rim_is_grey_all_round() {
        let (image, body) = shot(Style::Island, Preview::Attracted);
        let (middle, center) = (body.center().x, body.center().y);
        assert_eq!(pixel(&image, middle, body.bottom() - 2.0), [0, 0, 0], "the body is black");
        for (x, y) in [
            (middle, body.bottom() + WIDTH * 0.5),
            (middle, body.top() - WIDTH * 0.5),
            (body.left() - WIDTH * 0.5, center),
            (body.right() + WIDTH * 0.5, center),
        ] {
            assert!(is_keyline(pixel(&image, x, y)), "a grey keyline just outside it at {x}, {y}");
        }
        let past = pixel(&image, middle, body.bottom() + WIDTH + 2.0);
        assert!(!is_keyline(past) && past != [0, 0, 0], "and the desk past it: {past:?}");

        let (image, body) = shot(Style::Notch, Preview::Attracted);
        let below = pixel(&image, body.center().x, body.bottom() + WIDTH * 0.5);
        assert!(!is_keyline(below) && below != [0, 0, 0], "a hardware notch ends straight onto the desk: {below:?}");
    }
}
