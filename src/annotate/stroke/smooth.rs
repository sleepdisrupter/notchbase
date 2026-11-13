//! Smooths pointer samples into a curve through their midpoints, cut finely enough to stay within a tenth of a pixel.

use egui::Pos2;

pub(super) const FLATNESS: f32 = 0.1;
pub(super) const HALFWAY: f32 = 0.5;
const SAME_SPOT: f32 = 0.05;
const FARTHEST: f32 = 1.0e6;
const DEEPEST_HALVING: u32 = 16;

pub(super) fn distinct(samples: &[Pos2]) -> Vec<Pos2> {
    let mut kept: Vec<Pos2> = Vec::with_capacity(samples.len());
    for &sample in samples.iter().filter(|sample| is_usable(**sample)) {
        if kept.last().is_none_or(|last| last.distance(sample) >= SAME_SPOT) {
            kept.push(sample);
        }
    }
    kept
}

fn is_usable(sample: Pos2) -> bool {
    sample.x.abs() <= FARTHEST && sample.y.abs() <= FARTHEST
}

pub(super) fn smooth(samples: &[Pos2]) -> Vec<Pos2> {
    let (Some(&first), Some(&last)) = (samples.first(), samples.last()) else { return Vec::new() };
    let mut points = vec![first];
    let mut from = first;
    for (index, pair) in samples.windows(2).enumerate() {
        let midpoint = between(pair[0], pair[1], HALFWAY);
        let control = if index == 0 { between(from, midpoint, HALFWAY) } else { pair[0] };
        bend(&mut points, from, control, midpoint);
        from = midpoint;
    }
    bend(&mut points, from, between(from, last, HALFWAY), last);
    points
}

pub(super) fn between(from: Pos2, to: Pos2, t: f32) -> Pos2 {
    from + (to - from) * t
}

fn bend(points: &mut Vec<Pos2>, from: Pos2, control: Pos2, to: Pos2) {
    halve(points, from, control, to, DEEPEST_HALVING);
}

fn halve(points: &mut Vec<Pos2>, from: Pos2, control: Pos2, to: Pos2, halvings_left: u32) {
    if halvings_left == 0 || sag(from, control, to) <= FLATNESS {
        if points.last() != Some(&to) {
            points.push(to);
        }
        return;
    }
    let (leaving, arriving) = (between(from, control, HALFWAY), between(control, to, HALFWAY));
    let middle = between(leaving, arriving, HALFWAY);
    halve(points, from, leaving, middle, halvings_left - 1);
    halve(points, middle, arriving, to, halvings_left - 1);
}

fn sag(from: Pos2, control: Pos2, to: Pos2) -> f32 {
    let chord = to - from;
    let length_squared = chord.length_sq();
    let travelled =
        if length_squared > 0.0 { ((control - from).dot(chord) / length_squared).clamp(0.0, 1.0) } else { 0.0 };
    control.distance(from + chord * travelled) / 2.0
}
