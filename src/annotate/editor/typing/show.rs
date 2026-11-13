//! Shows the open block: its letters from their own mask, an outline around it and the caret.

use std::mem;

use egui::{Color32, ColorImage, CornerRadius, Painter, Pos2, Rect, Stroke, StrokeKind, Vec2, pos2, vec2};

use super::super::{pen, picture};
use super::{Setter, Typing};
use crate::annotate::{Fit, Lettering};
use crate::ui::color;

const NAME: &str = "annotate-words";
const ROOM: Vec2 = vec2(6.0, 3.0);
const CORNER: u8 = 6;
const NARROWEST_IN_LINES: f32 = 1.6;
const OUTLINE: f32 = 0.6;
const CARET_HALO: f32 = 0.3;
const CARET_HALO_WIDTH: f32 = 3.2;
const CARET_WIDTH: f32 = 1.6;
const CHROME_REACH: f32 = 8.0;

impl Typing {
    pub(in crate::annotate::editor) fn show(&mut self, ctx: &egui::Context, setter: &mut Setter) {
        self.proof(setter);
        if !mem::take(&mut self.unshown) {
            return;
        }
        let mask = self.proof.as_ref().and_then(|proof| mask(proof.lettering()));
        match (self.texture.as_mut(), mask) {
            (Some(texture), Some(mask)) => texture.set(mask, picture::options()),
            (_, mask) => self.texture = mask.map(|mask| ctx.load_texture(NAME, mask, picture::options())),
        }
    }

    pub(in crate::annotate::editor) fn paint(&self, painter: &Painter, fit: Fit) {
        let Some(proof) = &self.proof else { return };
        let window = painter.clip_rect();
        if let Some(texture) = &self.texture {
            let inked = seen(fit, proof.lettering().area().to_rect());
            let on_the_picture = painter.with_clip_rect(fit.rect().intersect(window));
            on_the_picture.image(texture.id(), inked, picture::WHOLE, Color32::WHITE);
        }
        let around_the_picture = painter.with_clip_rect(reach(fit).intersect(window));
        if let Some(outline) = self.outline(fit) {
            for line in pen::halo_and_ring(OUTLINE) {
                around_the_picture.rect_stroke(outline, CornerRadius::same(CORNER), line, StrokeKind::Middle);
            }
        }
        let caret = in_reach(seen(fit, proof.caret(self.buffer.caret())), fit);
        for line in halo_and_bar(self.color) {
            around_the_picture.line_segment(caret, line);
        }
    }

    pub(super) fn outline_as_drawn(&self, fit: Fit) -> Option<Rect> {
        Some(self.outline(fit)?.intersect(reach(fit)))
    }

    fn outline(&self, fit: Fit) -> Option<Rect> {
        let frame = seen(fit, self.proof.as_ref()?.lettering().frame());
        let narrowest = frame.height() * NARROWEST_IN_LINES / self.lines() as f32;
        let roomy = Rect::from_min_size(frame.min, frame.size().max(vec2(narrowest, 0.0)));
        Some(roomy.expand2(ROOM))
    }

    fn lines(&self) -> usize {
        self.buffer.text().split('\n').count()
    }
}

fn seen(fit: Fit, pixels: Rect) -> Rect {
    Rect::from_min_max(fit.to_view(pixels.min), fit.to_view(pixels.max))
}

fn reach(fit: Fit) -> Rect {
    fit.rect().expand(CHROME_REACH)
}

fn in_reach(caret: Rect, fit: Fit) -> [Pos2; 2] {
    let drawn_whole = reach(fit).shrink(CARET_HALO_WIDTH / 2.0);
    let x = caret.center().x.max(drawn_whole.left()).min(drawn_whole.right());
    [pos2(x, caret.top()), pos2(x, caret.bottom())]
}

fn halo_and_bar(ink: Color32) -> [Stroke; 2] {
    let halo = color::ink_on(ink).gamma_multiply(CARET_HALO);
    [Stroke::new(CARET_HALO_WIDTH, halo), Stroke::new(CARET_WIDTH, ink)]
}

fn mask(lettering: &Lettering) -> Option<ColorImage> {
    let (area, levels) = (lettering.area(), lettering.coverage());
    let whole = !area.is_empty() && levels.len() == area.width * area.height;
    whole.then(|| ColorImage::new(area.size(), levels.iter().map(|level| tinted(lettering.color(), *level)).collect()))
}

fn tinted(ink: Color32, level: u8) -> Color32 {
    let [r, g, b] =
        [ink.r(), ink.g(), ink.b()].map(|channel| ((u32::from(channel) * u32::from(level) + 127) / 255) as u8);
    Color32::from_rgba_premultiplied(r, g, b, level)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotate::PixelRect;
    use crate::annotate::sheet::timing::{FIVE_K, quickest, timed};
    use crate::annotate::testing::lettering;

    const HALF_COVERED: u8 = 128;
    const BLOCKS: [(&str, [usize; 2]); 3] = [
        ("a few words at the default size", [640, 64]),
        ("two lines of the tallest letters", [3790, 691]),
        ("a block that covers a 5K screenshot", FIVE_K),
    ];

    #[test]
    #[ignore = "prints how long a block's mask takes to become a texture's pixels; run it by name with --release"]
    fn mask_to_texture_is_timed() {
        for (name, [width, height]) in BLOCKS {
            let area = PixelRect { x: 0, y: 0, width, height };
            let label = lettering(color::RED, area, vec![HALF_COVERED; width * height]);
            let ready = quickest(|| timed(|| mask(&label)));
            println!("{name}, a {width} x {height} mask: its pixels are ready to upload in {ready:.1} ms");
        }
    }

    #[test]
    fn mask_is_ink_weighed_by_coverage() {
        let area = PixelRect { x: 4, y: 2, width: 3, height: 1 };
        let shown = mask(&lettering(color::RED, area, vec![0, 128, 255])).expect("a mask");
        assert_eq!(shown.size, [3, 1]);
        assert_eq!(shown.pixels[0], Color32::TRANSPARENT);
        assert_eq!(shown.pixels[2], color::RED);
        let [r, g, b, a] = shown.pixels[1].to_array();
        assert_eq!((r, g, b, a), (118, 42, 45, 128), "half of garnet, premultiplied");
    }

    #[test]
    fn caret_is_ink_bar_on_faint_halo() {
        for ink in [color::RED, color::YELLOW, color::GREEN, color::PEARL, color::INK] {
            let [halo, bar] = halo_and_bar(ink);
            assert_eq!((bar.color, bar.width), (ink, CARET_WIDTH), "one bar, as the letters beside it");
            assert!(halo.width > bar.width && halo.color.a() < u8::MAX / 2, "{ink:?}: the halo is wider and faint");
            assert_ne!(color::is_light(halo.color.to_opaque()), color::is_light(ink), "{ink:?}");
        }
        assert_eq!(halo_and_bar(color::INK)[0].color, color::PEARL.gamma_multiply(CARET_HALO));
        assert_eq!(halo_and_bar(color::PEARL)[0].color, color::INK.gamma_multiply(CARET_HALO));
    }

    #[test]
    fn caret_past_the_side_rests_beside() {
        let fit = Fit::new([800, 400], Rect::from_min_size(pos2(50.0, 50.0), vec2(400.0, 200.0)), 2.0);
        let (picture, middle) = (fit.rect(), fit.rect().center().x);
        let caret_at = |x: f32| Rect::from_min_max(pos2(x, 80.0), pos2(x, 105.0));
        assert_eq!(in_reach(caret_at(middle), fit), [pos2(middle, 80.0), pos2(middle, 105.0)]);
        let [top, bottom] = in_reach(caret_at(picture.right() + 300.0), fit);
        assert!(top.x == bottom.x && top.x > picture.right(), "{top:?}");
        assert!(top.x + CARET_HALO_WIDTH / 2.0 <= picture.right() + CHROME_REACH, "none of it is clipped away");
        assert_eq!((top.y, bottom.y), (80.0, 105.0));
        let [top, _] = in_reach(caret_at(picture.left() - 300.0), fit);
        assert!(top.x < picture.left() && top.x - CARET_HALO_WIDTH / 2.0 >= picture.left() - CHROME_REACH);
    }

    #[test]
    fn inkless_words_have_no_mask() {
        assert!(mask(&Lettering::empty()).is_none());
        assert!(mask(&lettering(color::RED, PixelRect::EMPTY, Vec::new())).is_none());
    }
}
