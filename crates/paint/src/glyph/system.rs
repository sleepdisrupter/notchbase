//! System glyphs: battery, bolt, lock, download, cup, moon, search and the administrator's shield.

use std::f32::consts::{FRAC_1_SQRT_2, PI, TAU};

use egui::{Mesh, Pos2, Rect, Shape, Stroke, Vec2, vec2};

use super::Glyph;
use super::pen::{Pen, arrow};
use crate::canvas::Canvas;

const BOLT: [(f32, f32); 6] = [(0.1, -0.46), (-0.3, 0.06), (-0.04, 0.06), (-0.12, 0.46), (0.3, -0.08), (0.04, -0.08)];
const SHACKLE_ARC: usize = 16;
const SHACKLE_REACH: f32 = 0.2;
const LOCK_TOP: f32 = -0.08;
const CLOSED_SHACKLE: f32 = -0.12;
const OPEN_SHACKLE: f32 = -0.26;
const OPEN_LEG: f32 = -0.22;
const MOON: f32 = 0.42;
const BITE: f32 = 0.34;
const BITE_AT: Vec2 = vec2(0.2, -0.18);
const MOON_STEPS: usize = 24;
const LENS_AT: f32 = -0.07;
const LENS: f32 = 0.27;
const HANDLE_TO: f32 = 0.4;
const SHIELD_TOP: f32 = -0.42;
const SHIELD_SHOULDER: (f32, f32) = (0.34, -0.3);
const SHIELD_WAIST: f32 = 0.04;
const SHIELD_TIP: f32 = 0.44;
const SHIELD_CURVE: usize = 8;

pub(super) fn paint(canvas: &mut Canvas, g: Glyph, pen: &Pen) {
    let Pen { center, color, size: s, stroke, .. } = *pen;
    let p = |x: f32, y: f32| pen.p(x, y);
    match g {
        Glyph::Bolt => {
            let points = BOLT.map(|(x, y)| p(x, y));
            pen.fill(canvas, &points, center);
        }
        Glyph::Battery => {
            canvas.rect_stroke(Rect::from_center_size(p(-0.04, 0.0), vec2(0.8, 0.44) * s), 0.1 * s, stroke);
            canvas.rect(Rect::from_min_max(p(0.39, -0.09), p(0.47, 0.09)), 0.03 * s, color);
            canvas.rect(Rect::from_min_max(p(-0.33, -0.11), p(0.1, 0.11)), 0.04 * s, color);
        }
        Glyph::Lock => lock(canvas, pen, CLOSED_SHACKLE, LOCK_TOP),
        Glyph::Unlock => lock(canvas, pen, OPEN_SHACKLE, OPEN_LEG),
        Glyph::Download => {
            arrow(canvas, p(0.0, -0.44), p(0.0, 0.16), 0.2 * s, stroke);
            canvas.add(Shape::line_segment([p(-0.34, 0.4), p(0.34, 0.4)], stroke));
        }
        Glyph::Cup => {
            canvas.rect_stroke(Rect::from_min_max(p(-0.38, -0.1), p(0.18, 0.4)), 0.12 * s, stroke);
            let half = std::f32::consts::FRAC_PI_2;
            let handle: Vec<Pos2> = (0..=SHACKLE_ARC)
                .map(|step| {
                    let angle = -half + 2.0 * half * step as f32 / SHACKLE_ARC as f32;
                    p(0.18 + 0.17 * angle.cos(), 0.12 + 0.14 * angle.sin())
                })
                .collect();
            canvas.add(Shape::line(handle, stroke));
            for x in [-0.2, 0.0] {
                canvas.add(Shape::line_segment([p(x, -0.44), p(x, -0.26)], stroke));
            }
        }
        Glyph::Moon => moon(canvas, pen),
        Glyph::Search => {
            canvas.ring(p(LENS_AT, LENS_AT), LENS * s, stroke);
            let rim = LENS_AT + LENS * FRAC_1_SQRT_2;
            canvas.add(Shape::line_segment([p(rim, rim), p(HANDLE_TO, HANDLE_TO)], stroke));
        }
        Glyph::Shield => shield(canvas, pen),
        _ => {}
    }
}

fn lock(canvas: &mut Canvas, pen: &Pen, shackle: f32, left_leg: f32) {
    let Pen { size: s, stroke, .. } = *pen;
    let p = |x: f32, y: f32| pen.p(x, y);
    canvas.rect(Rect::from_min_max(p(-0.32, -0.1), p(0.32, 0.42)), 0.1 * s, stroke.color);
    let pi = std::f32::consts::PI;
    let mut bow = vec![p(-SHACKLE_REACH, left_leg)];
    bow.extend((0..=SHACKLE_ARC).map(|step| {
        let angle = pi + pi * step as f32 / SHACKLE_ARC as f32;
        p(SHACKLE_REACH * angle.cos(), shackle + SHACKLE_REACH * angle.sin())
    }));
    bow.push(p(SHACKLE_REACH, LOCK_TOP));
    canvas.add(Shape::line(bow, stroke));
}

/// The shield Windows puts on whatever runs as administrator: flat shoulders, straight sides, curving to a point.
fn shield(canvas: &mut Canvas, pen: &Pen) {
    let (width, shoulder) = SHIELD_SHOULDER;
    let side: Vec<(f32, f32)> = std::iter::once((width, shoulder))
        .chain((0..=SHIELD_CURVE).map(|step| {
            let t = step as f32 / SHIELD_CURVE as f32;
            let x = width * (1.0 - t * t);
            (x, SHIELD_WAIST + (SHIELD_TIP - SHIELD_WAIST) * t)
        }))
        .collect();
    let mut outline = vec![pen.p(0.0, SHIELD_TOP)];
    outline.extend(side.iter().map(|&(x, y)| pen.p(x, y)));
    outline.extend(side.iter().rev().skip(1).map(|&(x, y)| pen.p(-x, y)));
    canvas.add(Shape::closed_line(outline, pen.stroke));
    let half = [pen.p(0.0, SHIELD_TOP), pen.p(-width, shoulder), pen.p(-width, SHIELD_WAIST), pen.p(0.0, SHIELD_WAIST)];
    pen.fill(canvas, &half, pen.p(-width * 0.5, (SHIELD_TOP + SHIELD_WAIST) * 0.5));
}

fn moon(canvas: &mut Canvas, pen: &Pen) {
    let (color, p) = (pen.color, |v: Vec2| pen.at(v));
    let distance = BITE_AT.length();
    let along = (MOON * MOON - BITE * BITE + distance * distance) / (2.0 * distance);
    let rise = (MOON * MOON - along * along).max(0.0).sqrt();
    let toward = BITE_AT / distance;
    let across = vec2(-toward.y, toward.x);
    let tips = [toward * along + across * rise, toward * along - across * rise];
    let away = toward.y.atan2(toward.x) + PI;
    let outer = arc(Vec2::ZERO, MOON, tips, away);
    let inner = arc(BITE_AT, BITE, tips, away);
    let mut mesh = Mesh::default();
    for (outside, inside) in outer.iter().zip(&inner) {
        mesh.colored_vertex(p(*outside), color);
        mesh.colored_vertex(p(*inside), color);
    }
    for step in 0..MOON_STEPS as u32 {
        let first = step * 2;
        mesh.add_triangle(first, first + 1, first + 2);
        mesh.add_triangle(first + 1, first + 3, first + 2);
    }
    canvas.add(Shape::mesh(mesh));
    let outline: Vec<Pos2> = outer.iter().chain(inner.iter().rev()).map(|point| p(*point)).collect();
    canvas.add(Shape::closed_line(outline, Stroke::new(1.0, color)));
}

fn arc(center: Vec2, radius: f32, [from, to]: [Vec2; 2], through: f32) -> Vec<Vec2> {
    let angle = |point: Vec2| (point - center).y.atan2((point - center).x);
    let (start, end) = (angle(from), angle(to));
    let ahead = (end - start).rem_euclid(TAU);
    let sweep = if (through - start).rem_euclid(TAU) < ahead { ahead } else { ahead - TAU };
    (0..=MOON_STEPS)
        .map(|step| {
            let turn = start + sweep * step as f32 / MOON_STEPS as f32;
            center + vec2(turn.cos(), turn.sin()) * radius
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use egui::Color32;

    use super::*;

    #[test]
    fn magnifier_handle_starts_on_rim() {
        let ctx = egui::Context::default();
        let center = egui::pos2(50.0, 50.0);
        let mut shapes = Vec::new();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let mut canvas = Canvas::new(ui.painter());
            let pen = Pen::new(&canvas, center, 20.0, Color32::WHITE);
            paint(&mut canvas, Glyph::Search, &pen);
            shapes = canvas.shapes.clone();
        });
        output.textures_delta.clear();
        let [Shape::Circle(lens), Shape::LineSegment { points, .. }] = &shapes[..] else {
            panic!("a ring and a handle, not {} shapes", shapes.len());
        };
        assert_eq!(lens.fill, Color32::TRANSPARENT, "a ring, so the page shows through the lens");
        assert!(((points[0] - lens.center).length() - lens.radius).abs() < 1e-3, "the handle leaves from the rim");
        assert!(points[1].x > points[0].x && points[1].y > points[0].y, "down and to the right");
        let reach = (points[1] - center).abs().max_elem().max(lens.radius + (lens.center - center).abs().max_elem());
        assert!(reach <= 10.0, "inside its box: {reach}");
    }

    #[test]
    fn crescent_faces_away_from_bite() {
        let distance = BITE_AT.length();
        let along = (MOON * MOON - BITE * BITE + distance * distance) / (2.0 * distance);
        let rise = (MOON * MOON - along * along).sqrt();
        let toward = BITE_AT / distance;
        let across = vec2(-toward.y, toward.x);
        let tips = [toward * along + across * rise, toward * along - across * rise];
        let away = toward.y.atan2(toward.x) + PI;
        let outer = arc(Vec2::ZERO, MOON, tips, away);
        assert!((outer[0] - tips[0]).length() < 1e-4 && (outer[MOON_STEPS] - tips[1]).length() < 1e-4);
        let middle = outer[MOON_STEPS / 2];
        assert!(middle.dot(toward) < 0.0, "the lit edge faces away from the bite");
        let inner = arc(BITE_AT, BITE, tips, away);
        assert!(inner.iter().all(|point| point.length() <= MOON + 1e-4), "the bite stays inside the moon");
    }
}
