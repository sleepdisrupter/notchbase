//! Springs that follow a moving target, for the labels that track a selection or flip sides. The first target is
//! taken at once and later ones are sprung to.

use crate::ui::spring::{Spring, SpringConfig};

/// Steps a spring and snaps it home once it settles, so it ends exactly on its target.
pub(in crate::capture) fn settle(spring: &mut Spring, dt: f32) {
    spring.step(dt);
    if spring.is_settled() {
        spring.snap_to(spring.target);
    }
}

pub(in crate::capture) struct Glide {
    spring: Spring,
    placed: bool,
}

impl Glide {
    pub(in crate::capture) fn new(config: SpringConfig, precision: f32) -> Self {
        Self { spring: Spring::new(0.0, config, precision), placed: false }
    }

    pub(in crate::capture) fn reset(&mut self) {
        self.placed = false;
    }

    pub(in crate::capture) fn is_settled(&self) -> bool {
        self.spring.is_settled()
    }

    pub(in crate::capture) fn follow(&mut self, target: f32, dt: f32) -> f32 {
        if self.placed {
            self.spring.animate_to(target, self.spring.config);
        } else {
            self.spring.snap_to(target);
            self.placed = true;
        }
        settle(&mut self.spring, dt);
        self.spring.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    #[test]
    fn glide_jumps_then_springs() {
        let mut glide = Glide::new(SpringConfig::new(0.2, 1.0), 0.01);
        assert_eq!(glide.follow(40.0, DT), 40.0);
        assert!(glide.is_settled());
        let next = glide.follow(80.0, DT);
        assert!(next > 40.0 && next < 80.0, "{next}");
        assert!(!glide.is_settled());
        for _ in 0..240 {
            glide.follow(80.0, DT);
        }
        assert!(glide.is_settled());
        glide.reset();
        assert_eq!(glide.follow(10.0, DT), 10.0);
    }
}
