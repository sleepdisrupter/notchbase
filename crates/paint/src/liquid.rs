//! The notch's liquid bottom edge, pushed around by the six music bands.

use notchbase_spectrum::{BANDS, Bands};

use crate::spring::{Spring, SpringConfig};
use crate::wave::{RIPPLES, Ripple, Wave};

const JIGGLE: SpringConfig = SpringConfig::new(0.26, 0.38);
const PRECISION: f32 = 0.002;
const DRIFT: f32 = 0.025;
const SWAY: f32 = 0.22;
const REBOUND: f32 = -0.3; // how far a ripple may swing past flat as it settles
const AWAKE: f32 = 0.01;

struct Slot {
    at: f32,
    width: f32,
    band: usize,
    gain: f32,
}

// Where each ripple sits along the edge and which band drives it: bass in the middle, highs at the ends.
const SLOTS: [Slot; RIPPLES] = [
    Slot { at: 0.5, width: 0.16, band: 0, gain: 1.0 },
    Slot { at: 0.35, width: 0.09, band: 1, gain: 0.8 },
    Slot { at: 0.65, width: 0.09, band: 2, gain: 0.8 },
    Slot { at: 0.21, width: 0.075, band: 3, gain: 0.66 },
    Slot { at: 0.79, width: 0.075, band: 4, gain: 0.66 },
    Slot { at: 0.09, width: 0.06, band: 5, gain: 0.5 },
    Slot { at: 0.91, width: 0.06, band: 5, gain: 0.5 },
];

pub struct Liquid {
    levels: [Spring; BANDS],
    time: f32,
}

impl Default for Liquid {
    fn default() -> Self {
        Self { levels: [Spring::new(0.0, JIGGLE, PRECISION); BANDS], time: 0.0 }
    }
}

impl Liquid {
    pub fn set(&mut self, bands: Bands) {
        for (spring, level) in self.levels.iter_mut().zip(bands) {
            spring.animate_to(level.clamp(0.0, 1.0), JIGGLE);
        }
    }

    pub fn step(&mut self, dt: f32) {
        self.time = (self.time + dt) % 1000.0;
        for spring in &mut self.levels {
            spring.step(dt);
        }
    }

    pub fn snap(&mut self) {
        for spring in &mut self.levels {
            spring.snap_to(spring.target);
        }
    }

    pub fn is_moving(&self) -> bool {
        self.levels.iter().any(|spring| !spring.is_settled() || spring.target > AWAKE)
    }

    pub fn level(&self, band: usize) -> f32 {
        self.levels.get(band).map_or(0.0, |spring| spring.value.clamp(0.0, 1.0))
    }

    pub fn wave(&self, depth: f32) -> Wave {
        let t = self.time;
        let mut wave = Wave::default();
        for (i, (ripple, slot)) in wave.ripples.iter_mut().zip(&SLOTS).enumerate() {
            let phase = i as f32 * 1.7;
            let sway = 1.0 + SWAY * (t * (2.1 + 0.37 * i as f32) + phase).sin();
            let level = self.levels[slot.band].value.max(REBOUND);
            *ripple = Ripple {
                at: slot.at + DRIFT * (t * 0.9 + phase).sin(),
                width: slot.width,
                depth: depth * slot.gain * level * sway,
            };
        }
        wave
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beats_bulge_then_settle() {
        let mut liquid = Liquid::default();
        assert!(!liquid.is_moving());
        assert_eq!(liquid.wave(6.0).deepest(), 0.0);
        liquid.set([1.0, 0.5, 0.5, 0.3, 0.3, 0.2]);
        for _ in 0..30 {
            liquid.step(1.0 / 60.0);
        }
        let wave = liquid.wave(6.0);
        assert!(wave.depth_at(0.5) > 3.0, "{}", wave.depth_at(0.5));
        assert!(liquid.is_moving());
        liquid.set([0.0; BANDS]);
        for _ in 0..600 {
            liquid.step(1.0 / 60.0);
        }
        assert!(!liquid.is_moving());
        assert!(liquid.wave(6.0).depth_at(0.5).abs() < 0.05);
    }

    #[test]
    fn hit_overshoots_like_liquid() {
        let mut liquid = Liquid::default();
        liquid.set([1.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
        let mut highest = 0.0f32;
        for _ in 0..60 {
            liquid.step(1.0 / 60.0);
            highest = highest.max(liquid.levels[0].value);
        }
        assert!(highest > 1.05, "no wobble: {highest}");
        assert_eq!(liquid.level(0), liquid.levels[0].value.clamp(0.0, 1.0));
        assert_eq!(liquid.level(BANDS), 0.0);
    }
}
