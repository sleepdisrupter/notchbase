//! A damped spring for animations. It's solved exactly, so it moves the same at any frame rate.

use std::f32::consts::TAU;

// How close counts as settled, for 0 to 1 values and for values in points.
pub const UNIT_PRECISION: f32 = 0.001;
pub const POINT_PRECISION: f32 = 0.05;

/// `response` is the undamped period in seconds. `damping` is the damping ratio: 1 settles
/// without overshoot, less bounces.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpringConfig {
    pub response: f32,
    pub damping: f32,
}

impl SpringConfig {
    pub const fn new(response: f32, damping: f32) -> Self {
        Self { response, damping }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Spring {
    pub value: f32,
    pub velocity: f32,
    pub target: f32,
    pub config: SpringConfig,
    precision: f32,
}

impl Spring {
    pub fn new(value: f32, config: SpringConfig, precision: f32) -> Self {
        Self { value, velocity: 0.0, target: value, config, precision }
    }

    pub fn animate_to(&mut self, target: f32, config: SpringConfig) {
        self.target = target;
        self.config = config;
    }

    pub fn snap_to(&mut self, value: f32) {
        self.value = value;
        self.target = value;
        self.velocity = 0.0;
    }

    pub fn is_settled(&self) -> bool {
        (self.value - self.target).abs() <= self.precision && self.velocity.abs() <= self.precision * 8.0
    }

    pub fn step(&mut self, dt: f32) {
        if self.is_settled() {
            self.value = self.target;
            self.velocity = 0.0;
            return;
        }

        let omega = TAU / self.config.response.max(1e-3);
        let zeta = self.config.damping.max(0.0);
        let x0 = self.value - self.target;
        let v0 = self.velocity;

        let (x, v) = if zeta < 0.999 {
            // underdamped
            let wd = omega * (1.0 - zeta * zeta).sqrt();
            let decay = (-zeta * omega * dt).exp();
            let a = x0;
            let b = (v0 + zeta * omega * x0) / wd;
            let (s, c) = (wd * dt).sin_cos();
            (decay * (a * c + b * s), decay * ((b * wd - zeta * omega * a) * c - (a * wd + zeta * omega * b) * s))
        } else if zeta > 1.001 {
            // overdamped
            let root = (zeta * zeta - 1.0).sqrt();
            let r1 = -omega * (zeta - root);
            let r2 = -omega * (zeta + root);
            let c2 = (v0 - r1 * x0) / (r2 - r1);
            let c1 = x0 - c2;
            let (e1, e2) = ((r1 * dt).exp(), (r2 * dt).exp());
            (c1 * e1 + c2 * e2, c1 * r1 * e1 + c2 * r2 * e2)
        } else {
            // critically damped
            let decay = (-omega * dt).exp();
            let b = v0 + omega * x0;
            ((x0 + b * dt) * decay, (b - omega * (x0 + b * dt)) * decay)
        };

        self.value = self.target + x;
        self.velocity = v;
    }
}

pub fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settle(mut s: Spring) -> Spring {
        for _ in 0..600 {
            s.step(1.0 / 120.0);
        }
        s
    }

    #[test]
    fn every_damping_regime_reaches_target() {
        for damping in [0.5, 1.0, 1.6] {
            let mut s = Spring::new(0.0, SpringConfig::new(0.4, damping), 0.01);
            s.animate_to(100.0, SpringConfig::new(0.4, damping));
            let s = settle(s);
            assert!((s.value - 100.0).abs() < 0.01, "damping {damping}: {}", s.value);
        }
    }

    #[test]
    fn only_underdamped_overshoots() {
        let mut bouncy = Spring::new(0.0, SpringConfig::new(0.4, 0.5), 0.01);
        bouncy.animate_to(1.0, bouncy.config);
        let mut stiff = Spring::new(0.0, SpringConfig::new(0.4, 1.0), 0.01);
        stiff.animate_to(1.0, stiff.config);
        let (mut max_b, mut max_s) = (0.0f32, 0.0f32);
        for _ in 0..240 {
            bouncy.step(1.0 / 120.0);
            stiff.step(1.0 / 120.0);
            max_b = max_b.max(bouncy.value);
            max_s = max_s.max(stiff.value);
        }
        assert!(max_b > 1.05);
        assert!(max_s <= 1.0 + 1e-4);
    }

    #[test]
    fn frame_rate_independent() {
        let cfg = SpringConfig::new(0.35, 0.7);
        let mut a = Spring::new(0.0, cfg, 0.001);
        let mut b = Spring::new(0.0, cfg, 0.001);
        a.animate_to(50.0, cfg);
        b.animate_to(50.0, cfg);
        for _ in 0..30 {
            a.step(1.0 / 60.0);
        }
        for _ in 0..60 {
            b.step(1.0 / 120.0);
        }
        assert!((a.value - b.value).abs() < 1e-2);
    }
}
