//! The capsule bars, the shimmer and the refresh glyph the usage card paints.

use std::f32::consts::{FRAC_PI_2, TAU};

use egui::{Color32, Mesh, Pos2, Rect, Shape, Stroke, pos2, vec2};

use crate::ui::canvas::Canvas;
use crate::ui::color;
use crate::ui::spring::smoothstep;

const MIN_FILL: f32 = 0.002;
const SHIMMER_BAND: f32 = 34.0;
const SHIMMER_ALPHA: f32 = 0.16;
const GLYPH_POINTS: usize = 28;

pub(super) fn capsule_track(canvas: &mut Canvas, bar: Rect) {
    canvas.rect(bar, bar.height() * 0.5, color::quaternary_label());
}

pub(super) fn capsule_fill(canvas: &mut Canvas, bar: Rect, used: f32, color: Color32) {
    if used < MIN_FILL {
        return;
    }
    let width = (bar.width() * used).max(bar.height());
    canvas.rect(Rect::from_min_size(bar.min, vec2(width, bar.height())), bar.height() * 0.5, color);
}

pub(super) fn shimmer_band(canvas: &mut Canvas, bar: Rect, phase: f32) {
    let (x0, x1) = (bar.left() + bar.height() * 0.5, bar.right() - bar.height() * 0.5);
    if x1 <= x0 {
        return;
    }
    let travel = (x1 - x0) + 4.0 * SHIMMER_BAND;
    let peak = x0 - 2.0 * SHIMMER_BAND + travel * smoothstep(0.0, 1.0, phase);
    let (top, bottom) = (bar.top() + 0.75, bar.bottom() - 0.75);
    let columns = ((x1 - x0) / 3.0).ceil().max(1.0) as u32;
    let mut mesh = Mesh::default();
    for i in 0..=columns {
        let x = x0 + (x1 - x0) * i as f32 / columns as f32;
        let t = (x - peak) / SHIMMER_BAND;
        let color = color::label().gamma_multiply(SHIMMER_ALPHA * (-t * t).exp());
        mesh.colored_vertex(pos2(x, top), color);
        mesh.colored_vertex(pos2(x, bottom), color);
    }
    for i in 0..columns {
        let (a, b) = (2 * i, 2 * (i + 1));
        mesh.add_triangle(a, a + 1, b + 1);
        mesh.add_triangle(a, b + 1, b);
    }
    canvas.add(Shape::mesh(mesh));
}

pub(super) fn placeholder(canvas: &mut Canvas, rect: Rect, phase: f32) {
    let pulse = 0.5 - 0.5 * (phase * TAU).cos();
    canvas.rect(rect, rect.height() * 0.5, color::quaternary_label().gamma_multiply(0.6 + 0.5 * pulse));
}

pub(super) fn refresh_glyph(canvas: &mut Canvas, center: Pos2, size: f32, rotation: f32, color: Color32) {
    let r = size * 0.4;
    let width = (size * 0.12).max(1.2);
    let (a0, a1) = (rotation - FRAC_PI_2 + 0.55, rotation - FRAC_PI_2 + TAU - 0.25);
    let points: Vec<Pos2> = (0..=GLYPH_POINTS)
        .map(|i| {
            let a = a0 + (a1 - a0) * i as f32 / GLYPH_POINTS as f32;
            center + vec2(a.cos(), a.sin()) * r
        })
        .collect();
    canvas.add(Shape::line(points, Stroke::new(width, color)));
    let (radial, along) = (vec2(a1.cos(), a1.sin()), vec2(-a1.sin(), a1.cos()));
    let base = center + radial * r;
    let head = size * 0.2;
    canvas.add(Shape::convex_polygon(
        vec![base + along * head * 1.1, base + radial * head, base - radial * head],
        color,
        Stroke::NONE,
    ));
}
