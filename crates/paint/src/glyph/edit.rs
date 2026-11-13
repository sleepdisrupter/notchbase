//! Screenshot editor glyphs: undo, redo, brush and text.

use std::f32::consts::{FRAC_PI_2, PI, TAU};
use std::iter;

use egui::{Pos2, Shape, Stroke, vec2};

use super::Glyph;
use super::pen::{Pen, arrow_head};
use crate::canvas::Canvas;

const TIP: f32 = -0.38;
const TOP: f32 = -0.15;
const TURN_AT: f32 = 0.15;
const TURN: f32 = 0.22;
const TAIL: f32 = -0.16;
const HEAD: f32 = 0.2;
const TURN_STEPS: usize = 16;
const CAP: f32 = 0.3;
const CAP_LINE: f32 = -0.33;
const BASE_LINE: f32 = 0.33;
const SERIF: f32 = 0.11;
const FOOT: f32 = 0.13;
const SQUIGGLE_REACH: f32 = 0.43;
const SQUIGGLE_SWELL: f32 = 0.24;
const SQUIGGLE_CRESTS: f32 = 1.5;
const SQUIGGLE_STEPS: usize = 36;

pub(super) fn paint(canvas: &mut Canvas, g: Glyph, pen: &Pen) {
    let Pen { center, size: s, stroke, .. } = *pen;
    match g {
        Glyph::Brush => squiggle(canvas, center, s, stroke),
        Glyph::Text => letter(canvas, center, s, stroke),
        _ => turning_arrow(canvas, g, center, s, stroke),
    }
}

fn turning_arrow(canvas: &mut Canvas, g: Glyph, center: Pos2, s: f32, stroke: Stroke) {
    let facing = if g == Glyph::Redo { -1.0 } else { 1.0 };
    let p = |x: f32, y: f32| center + vec2(x * facing, y) * s;
    let turn = (0..=TURN_STEPS).map(|step| {
        let angle = -FRAC_PI_2 + PI * step as f32 / TURN_STEPS as f32;
        p(TURN_AT + TURN * angle.cos(), TOP + TURN + TURN * angle.sin())
    });
    let path = iter::once(p(TIP, TOP)).chain(turn).chain(iter::once(p(TAIL, TOP + 2.0 * TURN)));
    canvas.add(Shape::line(path.collect(), stroke));
    arrow_head(canvas, p(TIP, TOP), vec2(-facing, 0.0), HEAD * s, stroke);
}

fn squiggle(canvas: &mut Canvas, center: Pos2, s: f32, stroke: Stroke) {
    let path = (0..=SQUIGGLE_STEPS).map(|step| {
        let along = step as f32 / SQUIGGLE_STEPS as f32;
        let swell = (TAU * SQUIGGLE_CRESTS * along).sin() * SQUIGGLE_SWELL;
        center + vec2(SQUIGGLE_REACH * (2.0 * along - 1.0), -swell) * s
    });
    canvas.add(Shape::line(path.collect(), stroke));
}

fn letter(canvas: &mut Canvas, center: Pos2, s: f32, stroke: Stroke) {
    let p = |x: f32, y: f32| center + vec2(x, y) * s;
    let cap = vec![p(-CAP, CAP_LINE + SERIF), p(-CAP, CAP_LINE), p(CAP, CAP_LINE), p(CAP, CAP_LINE + SERIF)];
    canvas.add(Shape::line(cap, stroke));
    canvas.add(Shape::line_segment([p(0.0, CAP_LINE), p(0.0, BASE_LINE)], stroke));
    canvas.add(Shape::line_segment([p(-FOOT, BASE_LINE), p(FOOT, BASE_LINE)], stroke));
}

#[cfg(test)]
mod tests {
    use egui::{Color32, Painter, Rect, pos2};

    use super::*;

    const SIZE: f32 = 20.0;

    fn bounds(g: Glyph, center: Pos2) -> Rect {
        let painter = Painter::new(egui::Context::default(), egui::LayerId::background(), Rect::EVERYTHING);
        let mut canvas = Canvas::new(&painter);
        let pen = Pen::new(&canvas, center, SIZE, Color32::WHITE);
        paint(&mut canvas, g, &pen);
        canvas.shapes.iter().fold(Rect::NOTHING, |all, shape| all.union(shape.visual_bounding_rect()))
    }

    #[test]
    fn redo_mirrors_undo() {
        let center = pos2(50.0, 50.0);
        let (undo, redo) = (bounds(Glyph::Undo, center), bounds(Glyph::Redo, center));
        assert!((undo.left() - center.x + redo.right() - center.x).abs() < 1e-3);
        assert!((undo.right() - center.x + redo.left() - center.x).abs() < 1e-3);
        assert_eq!((undo.top(), undo.bottom()), (redo.top(), redo.bottom()));
    }

    #[test]
    fn arrows_are_centred() {
        let center = pos2(50.0, 50.0);
        let undo = bounds(Glyph::Undo, center);
        assert!((undo.center() - center).length() < 0.03 * SIZE, "{undo:?}");
        assert!(undo.width() <= SIZE && undo.height() <= SIZE);
    }

    #[test]
    fn brush_and_letter_are_centred() {
        let center = pos2(50.0, 50.0);
        for tool in [Glyph::Brush, Glyph::Text] {
            let drawn = bounds(tool, center);
            assert!((drawn.center() - center).length() < 0.03 * SIZE, "{tool:?}: {drawn:?}");
            assert!(drawn.width() <= SIZE && drawn.height() <= SIZE, "{tool:?}: {drawn:?}");
            assert!(drawn.width() > SIZE / 2.0 && drawn.height() > SIZE / 3.0, "{tool:?} fills its box: {drawn:?}");
        }
    }

    #[test]
    fn brush_is_wide_letter_is_tall() {
        let center = pos2(50.0, 50.0);
        let (brush, letter) = (bounds(Glyph::Brush, center), bounds(Glyph::Text, center));
        assert!(brush.width() > brush.height(), "{brush:?}");
        assert!(letter.height() > letter.width(), "{letter:?}");
    }
}
