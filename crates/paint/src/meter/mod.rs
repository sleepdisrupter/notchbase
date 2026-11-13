//! A live audio level that sloshes like liquid inside a slider.

mod liquid;

use std::f32::consts::TAU;

use crate::spring::{Spring, SpringConfig};

pub use liquid::{Tint, paint};

const SLOSH: SpringConfig = SpringConfig::new(0.3, 0.52);
const PRECISION: f32 = 0.002;
const QUIET: f32 = 1e-4;
const RANGE_DB: f32 = 30.0;
const AWAKE: f32 = 0.01;
const RIPPLE_SPEED: f32 = 2.2;
const RIPPLE_LIVELINESS: f32 = 5.0;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Reading {
    pub level: f32,
    pub velocity: f32,
    pub phase: f32,
}

impl Reading {
    pub fn is_visible(&self) -> bool {
        self.level > AWAKE
    }
}

pub struct Meter {
    level: Spring,
    phase: f32,
}

impl Default for Meter {
    fn default() -> Self {
        Self { level: Spring::new(0.0, SLOSH, PRECISION), phase: 0.0 }
    }
}

impl Meter {
    pub fn listen(&mut self, peak: f32) {
        self.level.animate_to(loudness(peak), SLOSH);
    }

    pub fn follow(&mut self, value: f32) {
        self.level.animate_to(value.clamp(0.0, 1.0), SLOSH);
    }

    pub fn snap(&mut self, value: f32) {
        self.level.snap_to(value.clamp(0.0, 1.0));
    }

    pub fn is_moving(&self) -> bool {
        !self.level.is_settled() || self.level.target > AWAKE
    }

    pub fn step(&mut self, dt: f32) -> bool {
        self.level.step(dt);
        let level = self.level.value.clamp(0.0, 1.0);
        self.phase = (self.phase + dt * (RIPPLE_SPEED + RIPPLE_LIVELINESS * level)) % (TAU * 100.0);
        !self.level.is_settled() || self.level.target > AWAKE
    }

    pub fn reading(&self) -> Reading {
        Reading { level: self.level.value.max(0.0), velocity: self.level.velocity, phase: self.phase }
    }
}

/// Peak amplitude to 0..1 on a 30 dB scale, which is closer to how loud it sounds.
pub fn loudness(peak: f32) -> f32 {
    if !peak.is_finite() || peak <= QUIET {
        return 0.0;
    }
    ((20.0 * peak.min(1.0).log10() + RANGE_DB) / RANGE_DB).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loudness_is_perceptual() {
        assert_eq!(loudness(0.0), 0.0);
        assert_eq!(loudness(f32::NAN), 0.0);
        assert_eq!(loudness(1.0), 1.0);
        assert_eq!(loudness(3.0), 1.0);
        let quiet = loudness(0.1);
        assert!(quiet > 0.3 && quiet < 0.4, "{quiet}");
        assert!((loudness(0.5) - 0.8).abs() < 0.01);
        assert_eq!(loudness(0.03), 0.0);
    }

    #[test]
    fn liquid_overshoots_then_settles() {
        let mut meter = Meter::default();
        meter.listen(0.5);
        let mut highest = 0.0f32;
        for _ in 0..60 {
            assert!(meter.step(1.0 / 60.0));
            highest = highest.max(meter.reading().level);
        }
        assert!(highest > 0.85, "no slosh: {highest}");
        meter.listen(0.0);
        let mut animating = true;
        for _ in 0..240 {
            animating = meter.step(1.0 / 60.0);
        }
        assert!(!animating);
        assert!(!meter.reading().is_visible());
    }
}
