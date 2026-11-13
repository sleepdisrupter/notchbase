//! Everything else: sun, record, pipette, sparkle, AirDrop, pins and more.

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI, TAU};

use egui::{Color32, Pos2, Rect, Shape, Stroke, Vec2, vec2};

use super::Glyph;
use super::pen::Pen;
use crate::canvas::Canvas;

pub const SPARKLES: [(f32, f32, f32); 2] = [(0.07, 0.07, 0.4), (-0.3, -0.3, 0.17)];
const SUN_RAYS: usize = 8;
const VIEWFINDER_SCALE: f32 = 13.0 / 15.0;
const SPARKLE_POINTS: usize = 48;
const SPARKLE_CURVE: f32 = 0.62;
const CAPSULE_ARC: usize = 10;
const AIRDROP_DOT: f32 = 0.1;
const AIRDROP_LOW: f32 = 0.02;
const AIRDROP_RINGS: [(f32, f32); 2] = [(0.24, 0.62), (0.43, 0.46)];
const STAR_POINTS: usize = 10;
const PIPETTE_ROUND: usize = 12;
const PIN: [[(f32, f32); 4]; 4] = [
    [(-0.24, -0.46), (0.24, -0.46), (0.24, -0.34), (-0.24, -0.34)],
    [(-0.13, -0.36), (0.13, -0.36), (0.16, -0.04), (-0.16, -0.04)],
    [(-0.16, -0.06), (0.16, -0.06), (0.32, 0.14), (-0.32, 0.14)],
    [(-0.05, 0.12), (0.05, 0.12), (0.02, 0.5), (-0.02, 0.5)],
];
const SLASH_REACH: f32 = 0.42;
const SLASH_GAP: f32 = 0.13;

pub(super) fn paint(canvas: &mut Canvas, g: Glyph, pen: &Pen) {
    let Pen { center, color, size: s, stroke, .. } = *pen;
    let p = |x: f32, y: f32| pen.p(x, y);
    match g {
        Glyph::Sun => {
            canvas.circle(center, 0.2 * s, color);
            for ray in 0..SUN_RAYS {
                let (sin, cos) = (ray as f32 * FRAC_PI_4).sin_cos();
                canvas.add(Shape::line_segment([p(0.33 * cos, 0.33 * sin), p(0.46 * cos, 0.46 * sin)], stroke));
            }
        }
        Glyph::Stop => canvas.rect(Rect::from_center_size(center, vec2(s, s)), 0.22 * s, color),
        Glyph::Record => canvas.circle(center, 0.4 * s, color),
        Glyph::Pipette => pipette(canvas, pen),
        Glyph::Viewfinder => viewfinder(canvas, center, s * VIEWFINDER_SCALE, color),
        Glyph::Star => {
            let points: Vec<Pos2> = (0..STAR_POINTS)
                .map(|i| {
                    let angle = -FRAC_PI_2 + PI * i as f32 / 5.0;
                    let radius = if i % 2 == 0 { 0.46 } else { 0.2 };
                    p(radius * angle.cos(), radius * angle.sin() + 0.03)
                })
                .collect();
            pen.fill(canvas, &points, p(0.0, 0.03));
        }
        Glyph::Grid => {
            for (x, y) in [(-0.21, -0.21), (0.21, -0.21), (-0.21, 0.21), (0.21, 0.21)] {
                canvas.rect(Rect::from_center_size(p(x, y), Vec2::splat(0.32 * s)), 0.09 * s, color);
            }
        }
        Glyph::Video => {
            canvas.rect(Rect::from_min_max(p(-0.46, -0.26), p(0.14, 0.26)), 0.1 * s, color);
            let lens = [p(0.2, -0.04), p(0.46, -0.24), p(0.46, 0.24), p(0.2, 0.04)];
            pen.fill(canvas, &lens, p(0.36, 0.0));
        }
        Glyph::Cloud => {
            for (x, y, r) in [(-0.2, 0.06, 0.2), (0.04, -0.08, 0.27), (0.27, 0.08, 0.17)] {
                canvas.circle(p(x, y), r * s, color);
            }
            canvas.rect(Rect::from_min_max(p(-0.2, 0.04), p(0.27, 0.26)), 0.0, color);
        }
        Glyph::Link => {
            for (x, y) in [(-0.15, 0.15), (0.15, -0.15)] {
                canvas.add(Shape::closed_line(capsule(p(x, y), 0.17 * s, 0.13 * s), stroke));
            }
        }
        Glyph::Gauge => {
            canvas.add(Shape::line(pen.arc(0.0, 0.12, 0.42, 0.8 * PI, 2.2 * PI), stroke));
            canvas.add(Shape::line_segment([p(0.0, 0.12), p(0.2, -0.1)], stroke));
            canvas.circle(p(0.0, 0.12), 0.08 * s, color);
        }
        Glyph::AirDrop => {
            canvas.circle(p(0.0, AIRDROP_LOW), AIRDROP_DOT * s, color);
            for (radius, gap) in AIRDROP_RINGS {
                let (from, to) = (FRAC_PI_2 + gap, FRAC_PI_2 + TAU - gap);
                canvas.add(Shape::line(pen.arc(0.0, AIRDROP_LOW, radius, from, to), stroke));
            }
        }
        Glyph::Pin => pin(canvas, pen, None),
        Glyph::Unpin => {
            pin(canvas, pen, Some(vec2(1.0, -1.0).normalized()));
            canvas.add(Shape::line_segment([p(-SLASH_REACH, -SLASH_REACH), p(SLASH_REACH, SLASH_REACH)], stroke));
        }
        Glyph::Spark => {
            for (x, y, reach) in SPARKLES {
                let points = sparkle(p(x, y), reach * s);
                pen.fill(canvas, &points, p(x, y));
            }
        }
        _ => {}
    }
}

fn sparkle(center: Pos2, reach: f32) -> Vec<Pos2> {
    (0..SPARKLE_POINTS)
        .map(|step| {
            let angle = TAU * step as f32 / SPARKLE_POINTS as f32;
            let (cos, sin) = (angle.cos(), angle.sin());
            let bulge = (cos.abs().powf(SPARKLE_CURVE) + sin.abs().powf(SPARKLE_CURVE)).powf(-1.0 / SPARKLE_CURVE);
            center + vec2(cos, sin) * reach * bulge
        })
        .collect()
}

fn capsule(center: Pos2, reach: f32, radius: f32) -> Vec<Pos2> {
    let along = vec2(1.0, -1.0).normalized();
    let across = vec2(along.y, -along.x);
    let end = |sign: f32, start: f32| {
        (0..=CAPSULE_ARC).map(move |step| {
            let angle = start + PI * step as f32 / CAPSULE_ARC as f32;
            center + along * (sign * reach + radius * angle.cos()) + across * (radius * angle.sin())
        })
    };
    end(1.0, -PI * 0.5).chain(end(-1.0, PI * 0.5)).collect()
}

fn viewfinder(canvas: &mut Canvas, center: Pos2, size: f32, color: Color32) {
    let (half, arm) = (size * 0.5, size * 0.3);
    let stroke = Stroke::new((size * 0.11).max(1.3), color);
    for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        let corner = center + vec2(sx * half, sy * half);
        let points = vec![corner - vec2(0.0, sy * arm), corner, corner - vec2(sx * arm, 0.0)];
        canvas.add(Shape::line(points, stroke));
    }
    canvas.circle(center, size * 0.1, color);
}

fn pin(canvas: &mut Canvas, pen: &Pen, struck_across: Option<Vec2>) {
    for part in PIN {
        let part = part.map(|(x, y)| vec2(x, y));
        let pieces = match struck_across {
            Some(across) => vec![beyond(&part, across, SLASH_GAP), beyond(&part, -across, SLASH_GAP)],
            None => vec![part.to_vec()],
        };
        for piece in pieces.into_iter().filter(|piece| piece.len() >= 3) {
            let middle = piece.iter().fold(Vec2::ZERO, |sum, at| sum + *at) / piece.len() as f32;
            let points: Vec<Pos2> = piece.iter().map(|at| pen.at(*at)).collect();
            pen.fill(canvas, &points, pen.at(middle));
        }
    }
}

fn beyond(points: &[Vec2], across: Vec2, from: f32) -> Vec<Vec2> {
    let past = |at: Vec2| at.dot(across) - from;
    let mut kept = Vec::new();
    for (index, &a) in points.iter().enumerate() {
        let b = points[(index + 1) % points.len()];
        let (da, db) = (past(a), past(b));
        if da >= 0.0 {
            kept.push(a);
        }
        if (da >= 0.0) != (db >= 0.0) {
            kept.push(a + (b - a) * (da / (da - db)));
        }
    }
    kept
}

fn pipette(canvas: &mut Canvas, pen: &Pen) {
    let axis = vec2(1.0, -1.0).normalized();
    let across = vec2(-axis.y, axis.x);
    let scale = pen.size * 0.52;
    let map = |u: f32, v: f32| pen.center + (axis * u + across * v) * scale;

    let mut pts = vec![
        map(-1.0, 0.0),
        map(-0.78, -0.085),
        map(0.16, -0.085),
        map(0.16, -0.28),
        map(0.34, -0.28),
        map(0.34, -0.19),
        map(0.76, -0.19),
    ];
    for i in 1..PIPETTE_ROUND {
        let a = -FRAC_PI_2 + PI * i as f32 / PIPETTE_ROUND as f32;
        pts.push(map(0.76 + 0.19 * a.cos(), 0.19 * a.sin()));
    }
    pts.extend([
        map(0.76, 0.19),
        map(0.34, 0.19),
        map(0.34, 0.28),
        map(0.16, 0.28),
        map(0.16, 0.085),
        map(-0.78, 0.085),
    ]);
    pen.fill(canvas, &pts, map(0.25, 0.0));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparkle_is_a_four_pointed_star() {
        let points = sparkle(Pos2::ZERO, 10.0);
        assert_eq!(points.len(), SPARKLE_POINTS);
        for tip in [0, SPARKLE_POINTS / 4, SPARKLE_POINTS / 2, 3 * SPARKLE_POINTS / 4] {
            assert!((points[tip].to_vec2().length() - 10.0).abs() < 1e-3, "{tip}: {:?}", points[tip]);
        }
        let waist = points[SPARKLE_POINTS / 8].to_vec2().length();
        assert!(waist < 5.0, "the sides curve in between the tips: {waist}");
    }

    #[test]
    fn capsule_runs_along_the_diagonal() {
        let points = capsule(Pos2::ZERO, 4.0, 1.0);
        assert_eq!(points.len(), 2 * (CAPSULE_ARC + 1));
        let reach = points.iter().map(|point| point.to_vec2().length()).fold(0.0, f32::max);
        assert!((reach - 5.0).abs() < 1e-3, "{reach}");
        assert!(points.iter().all(|point| (point.x + point.y).abs() <= 2.0_f32.sqrt() + 1e-4));
    }
}
