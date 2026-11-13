//! The liquid selection marker. A leading point runs ahead of a trailing one, stretching the drop as it moves.

use std::f32::consts::{FRAC_PI_2, PI};

use egui::{Color32, Id, Pos2, Shape, Stroke, vec2};

use crate::ui::canvas::Canvas;
use crate::ui::spring::lerp;

const LEAD: f32 = 0.14;
const TRAIL: f32 = 0.36;
const FRONT: f32 = 0.88;
const BACK: f32 = 0.58;
const REACH: f32 = 5.0;
const ARC: usize = 16;
const SLAB_PINCH: f32 = 10.0;
const SLAB_RADIUS: f32 = 12.0;

pub(super) fn follow(ctx: &egui::Context, id: Id, target: Pos2) -> (Pos2, Pos2) {
    let at = |axis: &str, value: f32, time: f32| ctx.animate_value_with_time(id.with(axis), value, time);
    let lead = Pos2::new(at("lead x", target.x, LEAD), at("lead y", target.y, LEAD));
    let trail = Pos2::new(at("trail x", target.x, TRAIL), at("trail y", target.y, TRAIL));
    (trail, lead)
}

pub(super) fn drop(canvas: &mut Canvas, trail: Pos2, lead: Pos2, radius: f32, color: Color32) {
    let stretch = (trail.distance(lead) / (radius * REACH)).min(1.0);
    let front = radius * lerp(1.0, FRONT, stretch);
    let back = radius * lerp(1.0, BACK, stretch);
    canvas.add(Shape::convex_polygon(outline(trail, back, lead, front), color, Stroke::NONE));
}

pub(super) fn slab(canvas: &mut Canvas, column: egui::Rect, trail_x: f32, lead_x: f32, color: Color32) {
    let width = column.width();
    let (left, right) = (trail_x.min(lead_x) - width * 0.5, trail_x.max(lead_x) + width * 0.5);
    let stretch = ((right - left - width) / (width * REACH)).clamp(0.0, 1.0);
    let inset = SLAB_PINCH * stretch;
    let shape =
        egui::Rect::from_min_max(egui::pos2(left, column.top() + inset), egui::pos2(right, column.bottom() - inset));
    canvas.rect(shape, lerp(SLAB_RADIUS, width * 0.5, stretch), color);
}

fn outline(from: Pos2, from_radius: f32, to: Pos2, to_radius: f32) -> Vec<Pos2> {
    let heading = to - from;
    let angle = if heading.length() < 0.01 { 0.0 } else { heading.y.atan2(heading.x) };
    let arc = |center: Pos2, radius: f32, start: f32| {
        (0..=ARC).map(move |step| {
            let a = start + PI * step as f32 / ARC as f32;
            center + vec2(a.cos(), a.sin()) * radius
        })
    };
    arc(to, to_radius, angle - FRAC_PI_2).chain(arc(from, from_radius, angle + FRAC_PI_2)).collect()
}

#[cfg(test)]
mod tests {
    use egui::pos2;

    use super::*;

    #[test]
    fn resting_drop_is_a_circle() {
        let points = outline(pos2(10.0, 10.0), 5.0, pos2(10.0, 10.0), 5.0);
        assert!(points.iter().all(|point| (point.distance(pos2(10.0, 10.0)) - 5.0).abs() < 1e-3));
    }

    #[test]
    fn moving_drop_thins_behind() {
        let points = outline(pos2(0.0, 0.0), 3.0, pos2(20.0, 0.0), 5.0);
        let right = points.iter().map(|point| point.x).fold(f32::MIN, f32::max);
        let left = points.iter().map(|point| point.x).fold(f32::MAX, f32::min);
        assert!((right - 25.0).abs() < 1e-3 && (left + 3.0).abs() < 1e-3);
        let tallest = |x: f32| {
            points.iter().filter(|point| (point.x - x).abs() < 0.5).map(|point| point.y.abs()).fold(0.0, f32::max)
        };
        assert!(tallest(20.0) > tallest(0.0));
    }
}
