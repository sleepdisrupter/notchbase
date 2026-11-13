//! Ripples along the bottom edge of a shape, for the notch's music dance.

use std::f32::consts::PI;

pub const RIPPLES: usize = 7;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Ripple {
    pub at: f32,
    pub width: f32,
    pub depth: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Wave {
    pub ripples: [Ripple; RIPPLES],
}

impl Wave {
    /// How far the edge dips at `u`, from 0 at the left end to 1 at the right. It tapers off at
    /// both ends so the corners stay put.
    pub fn depth_at(&self, u: f32) -> f32 {
        let taper = (PI * u.clamp(0.0, 1.0)).sin().powi(2);
        let sum: f32 = self
            .ripples
            .iter()
            .filter(|ripple| ripple.width > 1e-3)
            .map(|ripple| ripple.depth * (-((u - ripple.at) / ripple.width).powi(2)).exp())
            .sum();
        taper * sum
    }

    pub fn deepest(&self) -> f32 {
        self.ripples.iter().map(|ripple| ripple.depth.max(0.0)).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wave(depth: f32) -> Wave {
        let mut wave = Wave::default();
        wave.ripples[0] = Ripple { at: 0.5, width: 0.2, depth };
        wave.ripples[1] = Ripple { at: 0.2, width: 0.1, depth: depth * 0.5 };
        wave.ripples[2] = Ripple { at: 0.8, width: 0.1, depth: depth * 0.5 };
        wave
    }

    #[test]
    fn edge_meets_corners_flat() {
        let w = wave(6.0);
        assert!(w.depth_at(0.0).abs() < 1e-5);
        assert!(w.depth_at(1.0).abs() < 1e-5);
        assert!((w.depth_at(0.5) - 6.0).abs() < 0.2);
        assert!((w.depth_at(0.3) - w.depth_at(0.7)).abs() < 1e-4, "mirrored ripples stay symmetric");
        assert!(w.depth_at(0.5) <= w.deepest());
    }

    #[test]
    fn still_wave_is_flat() {
        let flat = Wave::default();
        assert!((0..=10).all(|i| flat.depth_at(i as f32 / 10.0) == 0.0));
        assert_eq!(flat.deepest(), 0.0);
    }
}
