//! Spring settings shared across the UI, and a clock for waves and pulses that wraps so its floats stay precise.

use std::f32::consts::TAU;

use super::spring::SpringConfig;

pub const HOVER: SpringConfig = SpringConfig::new(0.18, 1.0);
pub const SMOOTH: SpringConfig = SpringConfig::new(0.32, 1.0);
pub const BOUNCY: SpringConfig = SpringConfig::new(0.42, 0.78);
pub const VALUE: SpringConfig = SpringConfig::new(0.5, 0.88);
pub const PRESS: SpringConfig = SpringConfig::new(0.2, 0.8);
pub const FOLLOW: SpringConfig = SpringConfig::new(0.14, 1.0);

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Clock(f32);

impl Clock {
    const WRAP_SECONDS: f32 = 1000.0;

    pub fn advance(&mut self, dt: f32) {
        self.0 = (self.0 + dt) % Self::WRAP_SECONDS;
    }

    pub fn wave(self, period: f32) -> f32 {
        (self.0 * TAU / period).sin()
    }

    pub fn seconds(self) -> f32 {
        self.0
    }

    pub fn pulse(self, period: f32) -> f32 {
        0.5 + 0.5 * self.wave(period)
    }

    pub fn phase(self, period: f32) -> f32 {
        (self.0 / period).fract()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_waves_and_pulses() {
        let mut clock = Clock::default();
        clock.advance(0.25);
        assert!((clock.wave(1.0) - 1.0).abs() < 1e-5);
        assert!((clock.pulse(1.0) - 1.0).abs() < 1e-5);
        clock.advance(0.5);
        assert!((clock.wave(1.0) + 1.0).abs() < 1e-4);
        assert!(clock.pulse(1.0).abs() < 1e-4);
    }

    #[test]
    fn clock_wraps_to_keep_precision() {
        let mut clock = Clock::default();
        clock.advance(999.5);
        clock.advance(1.0);
        assert!((clock.0 - 0.5).abs() < 1e-3);
    }
}
