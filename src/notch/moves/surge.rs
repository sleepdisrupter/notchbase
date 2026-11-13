//! The outward strain while a peek is held, just before it opens.

use std::f32::consts::TAU;
use std::time::{Duration, Instant};

use egui::{Pos2, Vec2, vec2};

use crate::notch::geometry::Silhouette;
use crate::notch::{Mode, Notch};
use crate::ui::shape;
use crate::ui::spring::SpringConfig;

pub(in crate::notch) const SURGE: Duration = Duration::from_millis(380);
pub(in crate::notch) const SURGE_OUT: SpringConfig = SpringConfig::new(0.3, 0.86);
const GROW: f32 = 4.5;
const BULGE: f32 = 8.0;
const WAVELENGTHS: [(f32, f32, f32); 2] = [(130.0, 0.6, 1.5), (76.0, 0.4, -2.1)];
const STEP: f32 = 3.0;
const SHAKE: f32 = 1.4;
const STRAIN: f32 = 1.7;
const SHOWN: f32 = 0.002;

impl Notch {
    pub(in crate::notch) fn surging(&self) -> bool {
        self.surge_progress(Instant::now()).is_some()
    }

    pub(super) fn surge_progress(&self, now: Instant) -> Option<f32> {
        if self.mode != Mode::Peek || self.preview.is_some() {
            return None;
        }
        let until = self.dwell_until?;
        let left = until.saturating_duration_since(now).as_secs_f32() / SURGE.as_secs_f32();
        (left < 1.0).then(|| (1.0 - left).clamp(0.0, 1.0))
    }

    pub(super) fn follow_surge(&mut self) {
        match self.surge_progress(Instant::now()) {
            Some(progress) => self.surge.snap_to(progress),
            None if self.surge.target != 0.0 => self.surge.animate_to(0.0, SURGE_OUT),
            None => {}
        }
    }

    pub(in crate::notch) fn surge_outline(&self, s: &Silhouette, pixels: f32) -> Option<(Vec<Pos2>, Pos2)> {
        let strain = self.surge.value.clamp(0.0, 1.0);
        if strain <= SHOWN {
            return None;
        }
        let points = resample(&shape::outline(&s.blob(), pixels));
        if points.len() < 3 {
            return None;
        }
        let eased = strain.powf(STRAIN);
        let seconds = self.clock.seconds();
        let shake = tremble(seconds) * SHAKE * eased * eased;
        let (walked, perimeter) = along(&points);
        let surge: Vec<Pos2> = points
            .iter()
            .zip(shape::vertex_normals(&points))
            .zip(walked)
            .map(|((&point, normal), at)| {
                point + shake + normal * (GROW * eased + BULGE * eased * lobes(at, perimeter, seconds))
            })
            .collect();
        Some((surge, s.rect().center() + shake))
    }
}

fn lobes(at: f32, perimeter: f32, seconds: f32) -> f32 {
    let wave: f32 = WAVELENGTHS
        .iter()
        .enumerate()
        .map(|(index, &(length, weight, speed))| {
            let count = (perimeter / length).round().max(1.0);
            weight * (count * at * TAU + speed * seconds + index as f32 * 1.3).sin()
        })
        .sum();
    (0.5 + 0.5 * wave).clamp(0.0, 1.0).powf(1.8)
}

pub(super) fn resample(points: &[Pos2]) -> Vec<Pos2> {
    let mut even = Vec::with_capacity(points.len() * 2);
    for (i, &from) in points.iter().enumerate() {
        let to = points[(i + 1) % points.len()];
        let pieces = (from.distance(to) / STEP).ceil().max(1.0) as usize;
        even.extend((0..pieces).map(|piece| from.lerp(to, piece as f32 / pieces as f32)));
    }
    even
}

fn tremble(seconds: f32) -> Vec2 {
    vec2(
        (seconds * TAU * 17.0).sin() + 0.5 * (seconds * TAU * 23.0 + 0.7).sin(),
        0.4 * (seconds * TAU * 19.0 + 1.9).sin(),
    )
}

pub(in crate::notch) fn along(points: &[Pos2]) -> (Vec<f32>, f32) {
    let mut walked = Vec::with_capacity(points.len());
    let mut total = 0.0;
    for (i, point) in points.iter().enumerate() {
        walked.push(total);
        total += point.distance(points[(i + 1) % points.len()]);
    }
    let shares = walked.into_iter().map(|at| if total > 0.0 { at / total } else { 0.0 }).collect();
    (shares, total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notch::tests::fixtures::{AWAY, OVER, island, ms, peek, settle};

    #[test]
    fn held_peek_strains_out() {
        let mut n = island();
        let t0 = peek(&mut n, Instant::now(), OVER);
        assert_eq!(n.mode, Mode::Peek);
        let delay = crate::config::current().notch.hover_delay();
        assert_eq!(n.surge_progress(t0 + delay - ms(20)), None, "the peek shows on its own first");
        let early = n.surge_progress(t0 + delay + ms(60)).expect("then it starts to strain");
        let late = n.surge_progress(t0 + delay + SURGE - ms(40)).expect("and strains harder");
        assert!(early < late && late < 1.0);
        n.tick(t0 + delay + SURGE - ms(10), Some(OVER));
        assert_eq!(n.mode, Mode::Peek, "it only opens once the strain peaks");
        n.tick(t0 + delay + SURGE + ms(5), Some(OVER));
        assert_eq!(n.mode, Mode::Expanded);
    }

    #[test]
    fn early_release_sinks_back() {
        let mut n = island();
        let t0 = peek(&mut n, Instant::now(), OVER);
        n.surge.snap_to(0.7);
        n.tick(t0 + ms(40), Some(AWAY));
        n.step(1.0 / 60.0);
        assert!(n.surge.value > 0.0 && n.surge.target == 0.0, "it sinks back on a spring, not in one frame");
        settle(&mut n);
        assert_eq!(n.surge.value, 0.0);
    }

    #[test]
    fn bulges_stay_outside() {
        for i in 0..64 {
            let value = lobes(i as f32 / 64.0, 600.0, i as f32 * 0.13);
            assert!((0.0..=1.0).contains(&value));
        }
        let square = [Pos2::new(0.0, 0.0), Pos2::new(10.0, 0.0), Pos2::new(10.0, 10.0), Pos2::new(0.0, 10.0)];
        assert_eq!(along(&square), (vec![0.0, 0.25, 0.5, 0.75], 40.0));
    }
}
