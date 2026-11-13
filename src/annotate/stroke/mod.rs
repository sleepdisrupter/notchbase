//! A finished line: colour, width and smoothed points, with bounds for each short run so inking skips what's clipped.

mod smooth;
#[cfg(test)]
mod tests;

use std::iter;
use std::ops::Range;

use egui::{Color32, Pos2, Rect};

use self::smooth::{distinct, smooth};

const ANTI_ALIAS: f32 = 1.0;
const THINNEST: f32 = 0.5;
const THICKEST: f32 = 512.0;
const RUN: usize = 4;

#[derive(Clone, Debug, PartialEq)]
pub struct Stroke {
    color: Color32,
    width: f32,
    points: Vec<Pos2>,
    bounds: Rect,
    reaches: Vec<Rect>,
}

impl Stroke {
    pub fn from_samples(color: Color32, width: f32, samples: &[Pos2]) -> Option<Self> {
        let points = smooth(&distinct(samples));
        if points.is_empty() {
            return None;
        }
        let color = Color32::from_rgb(color.r(), color.g(), color.b());
        let width = drawable(width);
        let bounds = reach(&points, width);
        let reaches = runs(points.len()).map(|run| reach(&points[run], width)).collect();
        Some(Self { color, width, points, bounds, reaches })
    }

    pub fn color(&self) -> Color32 {
        self.color
    }

    pub fn width(&self) -> f32 {
        self.width
    }

    pub fn points(&self) -> &[Pos2] {
        &self.points
    }

    pub fn bounds(&self) -> Rect {
        self.bounds
    }

    pub(super) fn radius(&self) -> f32 {
        self.width / 2.0
    }

    pub(super) fn runs(&self) -> impl Iterator<Item = (Rect, &[Pos2])> + '_ {
        iter::zip(&self.reaches, runs(self.points.len())).map(|(reach, run)| (*reach, &self.points[run]))
    }
}

fn drawable(width: f32) -> f32 {
    if width.is_nan() { THINNEST } else { width.clamp(THINNEST, THICKEST) }
}

fn reach(points: &[Pos2], width: f32) -> Rect {
    Rect::from_points(points).expand(width / 2.0 + ANTI_ALIAS)
}

fn runs(points: usize) -> impl Iterator<Item = Range<usize>> {
    let segments = points.saturating_sub(1).max(1);
    (0..segments).step_by(RUN).map(move |start| start..(start + RUN + 1).min(points))
}
