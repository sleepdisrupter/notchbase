//! Rounded rectangles as a path from the top center round and back, and its first part, for progress around a tile.

use std::f32::consts::{FRAC_PI_2, PI};

use egui::{Pos2, Rect, pos2, vec2};

const CORNER_SEGMENTS: usize = 10;

pub fn rounded_rect_loop(rect: Rect, radius: f32) -> Vec<Pos2> {
    let r = radius.clamp(0.0, 0.5 * rect.width().min(rect.height()));
    let top_center = pos2(rect.center().x, rect.top());
    let corners = [
        (pos2(rect.right() - r, rect.top() + r), -FRAC_PI_2),
        (pos2(rect.right() - r, rect.bottom() - r), 0.0),
        (pos2(rect.left() + r, rect.bottom() - r), FRAC_PI_2),
        (pos2(rect.left() + r, rect.top() + r), PI),
    ];
    let mut points = Vec::with_capacity(4 * (CORNER_SEGMENTS + 1) + 2);
    points.push(top_center);
    for (center, start) in corners {
        for i in 0..=CORNER_SEGMENTS {
            let angle = start + FRAC_PI_2 * i as f32 / CORNER_SEGMENTS as f32;
            points.push(center + vec2(angle.cos(), angle.sin()) * r);
        }
    }
    points.push(top_center);
    points
}

pub fn leading_part(points: &[Pos2], fraction: f32) -> Vec<Pos2> {
    let total: f32 = points.windows(2).map(|pair| pair[0].distance(pair[1])).sum();
    let mut remaining = total * fraction.clamp(0.0, 1.0);
    let mut part: Vec<Pos2> = points.first().copied().into_iter().collect();
    for pair in points.windows(2) {
        let length = pair[0].distance(pair[1]);
        if remaining >= length {
            part.push(pair[1]);
            remaining -= length;
        } else {
            if length > 0.0 {
                part.push(pair[0] + (pair[1] - pair[0]) * (remaining / length));
            }
            break;
        }
    }
    part
}

#[cfg(test)]
mod tests {
    use super::*;

    fn length(points: &[Pos2]) -> f32 {
        points.windows(2).map(|pair| pair[0].distance(pair[1])).sum()
    }

    #[test]
    fn loop_starts_and_ends_at_top() {
        let rect = Rect::from_min_size(pos2(10.0, 20.0), vec2(40.0, 30.0));
        let points = rounded_rect_loop(rect, 6.0);
        assert_eq!(points.first(), Some(&pos2(30.0, 20.0)));
        assert_eq!(points.last(), Some(&pos2(30.0, 20.0)));
        assert!(points.iter().all(|p| rect.expand(0.01).contains(*p)));
    }

    #[test]
    fn loop_length_is_the_perimeter() {
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(40.0, 30.0));
        assert!((length(&rounded_rect_loop(rect, 0.0)) - 140.0).abs() < 1e-3);
    }

    #[test]
    fn leading_part_covers_its_share() {
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(40.0, 30.0));
        let full = rounded_rect_loop(rect, 8.0);
        let total = length(&full);
        for fraction in [0.0, 0.1, 0.5, 0.93, 1.0] {
            assert!((length(&leading_part(&full, fraction)) - total * fraction).abs() < 1e-2);
        }
        assert!(leading_part(&[], 0.5).is_empty());
    }
}
