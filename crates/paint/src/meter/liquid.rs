//! Paints a meter reading as liquid inside a capsule track.

use std::f32::consts::TAU;

use egui::{Color32, Mesh, Pos2, Rect, Shape, Stroke, pos2};

use super::Reading;
use crate::canvas::Canvas;
use crate::color;
use crate::spring::{lerp, smoothstep};

const LEAN_PER_SPEED: f32 = 0.35;
const RIPPLE_SHARE: f32 = 0.16;
const EDGE_POINTS: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tint {
    pub color: Color32,
    pub tail: f32,
    pub head: f32,
    pub shine: f32,
}

impl Tint {
    pub const LEVEL: Tint = Tint { color: color::ACCENT, tail: 0.1, head: 0.45, shine: 0.85 };

    pub fn faded(self, strength: f32) -> Self {
        Self { tail: self.tail * strength, head: self.head * strength, shine: self.shine * strength, ..self }
    }
}

pub fn paint(canvas: &mut Canvas, track: Rect, span: f32, reading: Reading, reach: f32, tint: Tint) {
    let h = track.height();
    let r = h * 0.5;
    let full = span * reading.level;
    if full < r * 0.6 || reach < h {
        return;
    }
    let edge: Vec<Pos2> =
        liquid_edge(track, full, reading).into_iter().map(|p| clamp_to_capsule(p, track, reach)).collect();
    let left = track.left();
    let shade = |x: f32| {
        let t = ((x - left) / full).clamp(0.0, 1.0);
        tint.color.gamma_multiply(lerp(tint.tail, tint.head, t * t))
    };
    let mut body = Mesh::default();
    for (i, p) in edge.iter().enumerate() {
        let dy = (p.y - track.center().y).clamp(-r, r);
        let start = left + r - (r * r - dy * dy).max(0.0).sqrt();
        let end = p.x.max(start);
        for x in [start, (start + end) * 0.5, end] {
            body.colored_vertex(pos2(x, p.y), shade(x));
        }
        if i > 0 {
            let (a, b) = (3 * (i as u32 - 1), 3 * i as u32);
            body.add_triangle(a, a + 1, b);
            body.add_triangle(a + 1, b + 1, b);
            body.add_triangle(a + 1, a + 2, b + 1);
            body.add_triangle(a + 2, b + 2, b + 1);
        }
    }
    canvas.add(Shape::mesh(body));
    let exposed = smoothstep(r * 0.3, r * 1.2, reach - full);
    if exposed > 0.01 && tint.shine > 0.0 {
        let width = (h * 0.08).clamp(1.0, 1.6);
        let inset = width * 0.5;
        let lip: Vec<Pos2> =
            edge.iter().map(|p| pos2(p.x, p.y.clamp(track.top() + inset, track.bottom() - inset))).collect();
        let shine = color::mix(tint.color, Color32::WHITE, 0.35).gamma_multiply(tint.shine * exposed);
        canvas.add(Shape::line(lip, Stroke::new(width, shine)));
    }
}

fn liquid_edge(track: Rect, x: f32, reading: Reading) -> Vec<Pos2> {
    let h = track.height();
    let lean = (reading.velocity * LEAN_PER_SPEED * h).clamp(-h * 0.6, h * 0.6);
    let ripple = h * RIPPLE_SHARE * smoothstep(0.0, 0.4, reading.level);
    (0..=EDGE_POINTS)
        .map(|i| {
            let t = i as f32 / EDGE_POINTS as f32;
            let wave = 0.6 * (TAU * 0.8 * t + reading.phase).sin() + 0.4 * (TAU * 1.6 * t - reading.phase * 1.3).sin();
            let y = lerp(track.top(), track.bottom(), t);
            pos2(track.left() + x + ripple * wave + lean * (0.5 - t), y)
        })
        .collect()
}

fn clamp_to_capsule(p: Pos2, track: Rect, reach: f32) -> Pos2 {
    let r = track.height() * 0.5;
    let cy = track.center().y;
    let dy = (p.y - cy).clamp(-r, r);
    let bulge = (r * r - dy * dy).max(0.0).sqrt();
    let (lo, hi) = (track.left() + r - bulge, track.left() + reach - r + bulge);
    pos2(p.x.clamp(lo, hi.max(lo)), p.y)
}

#[cfg(test)]
mod tests {
    use egui::vec2;

    use super::*;

    #[test]
    fn liquid_stays_inside_slider() {
        let track = Rect::from_min_size(pos2(10.0, 0.0), vec2(200.0, 28.0));
        for (level, velocity) in [(0.5, 0.0), (1.2, 6.0), (0.95, -6.0), (0.2, 3.0)] {
            let reading = Reading { level, velocity, phase: 1.0 };
            for p in liquid_edge(track, 200.0 * level, reading).into_iter().map(|p| clamp_to_capsule(p, track, 120.0)) {
                let inside_x = p.x >= track.left() - 0.01 && p.x <= track.left() + 120.0 + 0.01;
                assert!(inside_x && track.y_range().contains(p.y), "{p:?}");
                let right_center = pos2(track.left() + 120.0 - 14.0, track.center().y);
                assert!(p.x <= right_center.x || p.distance(right_center) <= 14.01, "{p:?}");
            }
        }
    }
}
