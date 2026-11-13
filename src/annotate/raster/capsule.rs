//! One segment of a line: how much of a pixel it covers, and which rows and columns it can reach.

use std::ops::Range;

use egui::{Pos2, Vec2};

use super::{FULL, PIXEL_CENTRE};

const SOFT_EDGE: f32 = 0.5;
const CORE_MARGIN: f32 = 1.0 / 64.0;

pub(super) struct Capsule {
    from: Pos2,
    along: Vec2,
    per_length_squared: f32,
    lean: f32,
    radius: f32,
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
}

impl Capsule {
    pub(super) fn new(from: Pos2, to: Pos2, radius: f32) -> Self {
        let along = to - from;
        let length_squared = along.length_sq();
        Self {
            from,
            along,
            per_length_squared: if length_squared > 0.0 { length_squared.recip() } else { 0.0 },
            lean: if along.y == 0.0 { 0.0 } else { along.x / length_squared.sqrt() * along.y.signum() },
            radius,
            left: from.x.min(to.x),
            right: from.x.max(to.x),
            top: from.y.min(to.y),
            bottom: from.y.max(to.y),
        }
    }

    fn reach(&self) -> f32 {
        self.radius + SOFT_EDGE
    }

    pub(super) fn coverage(&self, centre: Pos2) -> u8 {
        let offset = centre - self.from;
        let travelled = (offset.dot(self.along) * self.per_length_squared).clamp(0.0, 1.0);
        let distance = (offset - self.along * travelled).length();
        ((self.reach() - distance).clamp(0.0, 1.0) * f32::from(FULL) + 0.5) as u8
    }

    pub(super) fn rows(&self, within: Range<usize>) -> Range<usize> {
        span(self.top - self.reach(), self.bottom + self.reach(), &within)
    }

    pub(super) fn is_beside(&self, columns: &Range<usize>) -> bool {
        self.right + self.reach() < columns.start as f32 || self.left - self.reach() > columns.end as f32
    }

    pub(super) fn core(&self, centre_y: f32, within: &Range<usize>) -> Range<usize> {
        let radius = self.radius - SOFT_EDGE - CORE_MARGIN;
        let down = centre_y - self.from.y;
        let tangent = radius * self.lean;
        let sides = self.side(down, tangent, radius, -1.0).zip(self.side(down, -tangent, radius, 1.0));
        let Some((first, last)) = sides else { return within.start..within.start };
        span((first - PIXEL_CENTRE).ceil(), (last - PIXEL_CENTRE).floor() + 1.0, within)
    }

    pub(super) fn nearest(&self, centre_y: f32) -> f32 {
        self.from.x + self.travelled_to(centre_y - self.from.y) * self.along.x
    }

    fn side(&self, down: f32, tangent: f32, radius: f32, outward: f32) -> Option<f32> {
        if self.along.y == 0.0 {
            let half = half_chord(radius, down.abs());
            let end = if outward < 0.0 { self.left } else { self.right };
            return (half > 0.0).then_some(end + outward * half);
        }
        let travelled = self.travelled_to(down - tangent);
        let half = half_chord(radius, (down - travelled * self.along.y).abs());
        (half > 0.0).then_some(self.from.x + travelled * self.along.x + outward * half)
    }

    fn travelled_to(&self, down: f32) -> f32 {
        if self.along.y == 0.0 { 0.0 } else { (down / self.along.y).clamp(0.0, 1.0) }
    }
}

fn half_chord(radius: f32, away: f32) -> f32 {
    if radius > away { (radius * radius - away * away).sqrt() } else { 0.0 }
}

fn span(from: f32, to: f32, within: &Range<usize>) -> Range<usize> {
    let start = (from.floor().max(0.0) as usize).clamp(within.start, within.end);
    let end = (to.ceil().max(0.0) as usize).clamp(start, within.end);
    start..end
}
