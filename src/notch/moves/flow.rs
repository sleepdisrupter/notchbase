//! The liquid outline while the notch runs: one side stretches and the surface dents away from the pointer.

use std::f32::consts::TAU;

use egui::{Pos2, Vec2, vec2};

use crate::notch::Notch;
use crate::notch::geometry::Silhouette;
use crate::ui::shape;

use super::surge::{along, resample};

const SHOWN: f32 = 0.002;
const SWIFT: f32 = 12.0;
const DENT: f32 = 9.0;
const DENT_REACH: f32 = 38.0;
const SMEAR: f32 = 0.055;
const SMEAR_MAX: f32 = 46.0;
const LEAD: f32 = 0.3;
const TAPER: f32 = 0.55;
const RIPPLE: f32 = 1.3;
const RIPPLE_WAVES: f32 = 7.0;
const RIPPLE_SPEED: f32 = 5.5;

impl Notch {
    pub(in crate::notch) fn flowing(&self) -> bool {
        self.fear.value > SHOWN || self.flee.velocity.abs() > SWIFT
    }

    pub(in crate::notch) fn flowing_outline(&self, s: &Silhouette, pixels: f32) -> Option<(Vec<Pos2>, Pos2)> {
        let points = resample(&shape::outline(&s.blob(), pixels));
        if points.len() < 3 {
            return None;
        }
        let rect = s.rect();
        let fear = self.fear.value.clamp(0.0, 1.0);
        let smear = (self.flee.velocity * SMEAR).clamp(-SMEAR_MAX, SMEAR_MAX);
        let seconds = self.clock.seconds();
        let (walked, _) = along(&points);
        let flowed: Vec<Pos2> = points
            .iter()
            .zip(walked)
            .map(|(&point, at)| {
                let across = ((point.x - rect.center().x) / (rect.width() * 0.5).max(1.0)).clamp(-1.0, 1.0);
                let depth = ((point.y - rect.top()) / rect.height().max(1.0)).clamp(0.0, 1.0);
                let trailed = trail(point, across, smear, rect.top());
                let ripple = RIPPLE * fear * depth * (RIPPLE_WAVES * at * TAU - RIPPLE_SPEED * seconds).sin();
                trailed + recoil(trailed, self.feared, fear) + vec2(0.0, ripple)
            })
            .collect();
        Some((flowed, rect.center() + vec2(0.0, -rect.height() * 0.25)))
    }
}

fn trail(point: Pos2, across: f32, smear: f32, top: f32) -> Pos2 {
    if smear == 0.0 {
        return point;
    }
    let heading = smear.signum();
    let behind = (-across * heading).max(0.0);
    let ahead = (across * heading).max(0.0);
    let dx = -smear * behind.powf(1.5) + smear * LEAD * ahead;
    let thin = TAPER * behind * behind * (smear.abs() / SMEAR_MAX);
    Pos2::new(point.x + dx, top + (point.y - top) * (1.0 - thin))
}

fn recoil(point: Pos2, feared: Option<Pos2>, fear: f32) -> Vec2 {
    let Some(from) = feared else { return Vec2::ZERO };
    let away = point - from;
    let distance = away.length();
    if distance < 1e-3 || fear <= 0.0 {
        return Vec2::ZERO;
    }
    let push = DENT * fear * (-(distance / DENT_REACH).powi(2)).exp();
    vec2(away.x / distance * push, (away.y / distance * push).min(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trailing_side_stretches() {
        let top = 0.0;
        let tail = trail(Pos2::new(100.0, 20.0), -1.0, 30.0, top);
        assert!(tail.x < 100.0 - 25.0, "moving right, the left edge lags behind");
        assert!(tail.y < 20.0, "and the tail tapers");
        let nose = trail(Pos2::new(200.0, 20.0), 1.0, 30.0, top);
        assert!(nose.x > 200.0 && nose.x < 200.0 + 30.0, "the leading edge leans ahead a little");
        assert_eq!(trail(Pos2::new(5.0, 5.0), 0.3, 0.0, top), Pos2::new(5.0, 5.0));
    }

    #[test]
    fn surface_dents_from_pointer() {
        let from = Some(Pos2::new(100.0, 40.0));
        let near = recoil(Pos2::new(110.0, 24.0), from, 1.0);
        assert!(near.x > 0.0 && near.y < 0.0, "pushed away from the pointer");
        let far = recoil(Pos2::new(300.0, 24.0), from, 1.0);
        assert!(far.length() < 0.01);
        let above = recoil(Pos2::new(100.0, 60.0), from, 1.0);
        assert_eq!(above.y, 0.0);
        assert_eq!(recoil(Pos2::new(110.0, 24.0), from, 0.0), Vec2::ZERO);
    }
}
