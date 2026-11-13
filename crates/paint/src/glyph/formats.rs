//! File kind glyphs: photo, camera, film, document, waveform and the like.

use std::f32::consts::TAU;

use egui::{Pos2, Rect, Shape, Vec2, vec2};

use super::Glyph;
use super::pen::Pen;
use crate::canvas::Canvas;

const RIDGE: [(f32, f32); 5] = [(-0.34, 0.24), (-0.1, -0.02), (0.08, 0.16), (0.18, 0.06), (0.34, 0.24)];
const SHEET: [(f32, f32); 4] = [(0.0, -0.42), (0.42, -0.2), (0.0, 0.02), (-0.42, -0.2)];
const PAGE: [(f32, f32); 5] = [(-0.3, -0.44), (0.1, -0.44), (0.32, -0.22), (0.32, 0.44), (-0.3, 0.44)];
const BARS: [f32; 5] = [0.26, 0.6, 0.88, 0.46, 0.2];
const BAR_STEP: f32 = 0.2;
const MERIDIAN: f32 = 0.18;
const ROUND_STEPS: usize = 24;
const WAVE_STEPS: usize = 32;
const WAVE_TURNS: f32 = 2.0;
const WAVE_REACH: f32 = 0.34;

pub(super) fn paint(canvas: &mut Canvas, g: Glyph, pen: &Pen) {
    let Pen { center, color, size: s, stroke, .. } = *pen;
    let p = |x: f32, y: f32| pen.p(x, y);
    let path = |points: &[(f32, f32)]| points.iter().map(|&(x, y)| p(x, y)).collect::<Vec<Pos2>>();
    match g {
        Glyph::Photo => {
            canvas.rect_stroke(Rect::from_min_max(p(-0.44, -0.36), p(0.44, 0.36)), 0.1 * s, stroke);
            canvas.circle(p(-0.2, -0.14), 0.08 * s, color);
            canvas.add(Shape::line(path(&RIDGE), stroke));
        }
        Glyph::Camera => {
            canvas.rect_stroke(Rect::from_min_max(p(-0.44, -0.24), p(0.44, 0.38)), 0.1 * s, stroke);
            canvas.rect(Rect::from_min_max(p(-0.16, -0.38), p(0.16, -0.24)), 0.04 * s, color);
            canvas.ring(p(0.0, 0.07), 0.15 * s, stroke);
        }
        Glyph::Globe => {
            canvas.ring(center, 0.42 * s, stroke);
            let meridian = (0..ROUND_STEPS).map(|step| {
                let (sin, cos) = (TAU * step as f32 / ROUND_STEPS as f32).sin_cos();
                p(MERIDIAN * cos, 0.42 * sin)
            });
            canvas.add(Shape::closed_line(meridian.collect(), stroke));
            canvas.add(Shape::line_segment([p(-0.42, 0.0), p(0.42, 0.0)], stroke));
        }
        Glyph::Layers => {
            pen.fill(canvas, &path(&SHEET), p(0.0, -0.2));
            for y in [0.02, 0.22] {
                canvas.add(Shape::line(vec![p(-0.42, y), p(0.0, y + 0.22), p(0.42, y)], stroke));
            }
        }
        Glyph::Frames => {
            canvas.rect_stroke(Rect::from_min_max(p(-0.44, -0.2), p(0.28, 0.42)), 0.1 * s, stroke);
            canvas.add(Shape::line(vec![p(-0.24, -0.4), p(0.46, -0.4), p(0.46, 0.2)], stroke));
            let play = [p(-0.18, -0.06), p(0.1, 0.11), p(-0.18, 0.28)];
            pen.fill(canvas, &play, p(-0.08, 0.11));
        }
        Glyph::AppIcon => {
            canvas.rect_stroke(Rect::from_center_size(center, Vec2::splat(0.8 * s)), 0.22 * s, stroke);
            canvas.circle(center, 0.14 * s, color);
        }
        Glyph::Document => {
            canvas.add(Shape::closed_line(path(&PAGE), stroke));
            canvas.add(Shape::line(vec![p(0.1, -0.44), p(0.1, -0.22), p(0.32, -0.22)], stroke));
            for y in [0.06, 0.24] {
                canvas.add(Shape::line_segment([p(-0.14, y), p(0.16, y)], stroke));
            }
        }
        Glyph::Film => {
            canvas.rect_stroke(Rect::from_min_max(p(-0.44, -0.34), p(0.44, 0.34)), 0.1 * s, stroke);
            for x in [-0.22, 0.22] {
                canvas.add(Shape::line_segment([p(x, -0.34), p(x, 0.34)], stroke));
                canvas.add(Shape::line_segment([p(x, 0.0), p(x + 0.22 * x.signum(), 0.0)], stroke));
            }
        }
        Glyph::Waveform => {
            let wave = (0..=WAVE_STEPS).map(|step| {
                let t = step as f32 / WAVE_STEPS as f32;
                let swell = (std::f32::consts::PI * t).sin();
                p(-0.46 + 0.92 * t, -WAVE_REACH * swell * (TAU * WAVE_TURNS * t).sin())
            });
            canvas.add(Shape::line(wave.collect(), stroke));
        }
        Glyph::Bars => {
            let first = -BAR_STEP * (BARS.len() - 1) as f32 * 0.5;
            for (i, height) in BARS.into_iter().enumerate() {
                let bar = Rect::from_center_size(p(first + BAR_STEP * i as f32, 0.0), vec2(stroke.width, height * s));
                canvas.rect(bar, stroke.width * 0.5, color);
            }
        }
        _ => {}
    }
}
