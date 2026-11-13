//! The Claude spark and the Codex knot, drawn from shapes so they can turn at any size.

use std::f32::consts::{PI, TAU};

use egui::{Color32, Pos2, Shape, Stroke, vec2};

use super::canvas::Canvas;

const RAYS: [f32; 12] = [1.0, 0.78, 0.93, 0.72, 0.97, 0.81, 0.99, 0.75, 0.9, 0.7, 0.95, 0.84];
const CORE: f32 = 0.11;
const RAY_BASE: f32 = 0.085;
const RAY_TIP: f32 = 0.028;
const PETALS: usize = 6;
const PETAL_OFFSET: f32 = 0.19;
const PETAL_RADIUS: f32 = 0.27;
const PETAL_SWEEP: f32 = 1.25 * PI;
const PETAL_STEPS: usize = 18;

pub fn spark(canvas: &mut Canvas, center: Pos2, size: f32, color: Color32, turn: f32) {
    let half = size * 0.5;
    for (index, reach) in RAYS.iter().enumerate() {
        let angle = turn + index as f32 * TAU / RAYS.len() as f32;
        let (along, across) = (vec2(angle.cos(), angle.sin()), vec2(-angle.sin(), angle.cos()));
        let (inner, outer) = (half * CORE, half * reach);
        let points = vec![
            center + along * inner + across * half * RAY_BASE,
            center + along * outer + across * half * RAY_TIP,
            center + along * outer - across * half * RAY_TIP,
            center + along * inner - across * half * RAY_BASE,
        ];
        canvas.add(Shape::convex_polygon(points, color, Stroke::NONE));
    }
    canvas.circle(center, half * CORE * 1.6, color);
}

pub fn knot(canvas: &mut Canvas, center: Pos2, size: f32, color: Color32, turn: f32) {
    let half = size * 0.5;
    let stroke = Stroke::new((size * 0.085).max(1.1), color);
    for petal in 0..PETALS {
        let angle = turn + petal as f32 * TAU / PETALS as f32;
        let hub = center + vec2(angle.cos(), angle.sin()) * half * PETAL_OFFSET * 2.0;
        let start = angle + PI * 0.35;
        let points: Vec<Pos2> = (0..=PETAL_STEPS)
            .map(|step| {
                let t = start + PETAL_SWEEP * step as f32 / PETAL_STEPS as f32;
                hub + vec2(t.cos(), t.sin()) * half * PETAL_RADIUS * 2.0 * 0.5
            })
            .collect();
        canvas.add(Shape::line(points, stroke));
    }
}
