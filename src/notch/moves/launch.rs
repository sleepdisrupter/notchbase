//! The one swell a closed notch gives when the app starts.

use std::time::{Duration, Instant};

use crate::notch::Notch;
use crate::ui::spring::SpringConfig;

const KICK: f32 = 150.0;
const SWELL: SpringConfig = SpringConfig::new(0.36, 0.52);
const CLOSED: f32 = 0.12;
const WAITS: Duration = Duration::from_millis(700);

impl Notch {
    pub fn swallow(&mut self) {
        if self.preview.is_none() {
            self.swallowing = Some(Instant::now() + WAITS);
        }
    }

    pub(super) fn follow_launch(&mut self) {
        let Some(until) = self.swallowing else { return };
        let closed = self.open_progress(self.morph.current()) <= CLOSED;
        if closed {
            self.gulp.velocity = KICK;
            self.gulp.animate_to(0.0, SWELL);
        }
        if closed || Instant::now() >= until {
            self.swallowing = None;
        }
    }

    pub(in crate::notch) fn launch_swell(&self, open: f32) -> f32 {
        self.gulp.value * (1.0 - open.clamp(0.0, 1.0))
    }

    pub(in crate::notch) fn launch_moves(&self) -> bool {
        self.swallowing.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::Page;
    use crate::notch::tests::fixtures::{island, notch, settle};

    #[test]
    fn swells_once_and_settles() {
        for mut n in [island(), notch()] {
            let rest = n.silhouette().h;
            n.swallow();
            assert!(n.is_animating(), "it has something to do");
            n.step(1.0 / 60.0);
            assert!(n.swallowing.is_none(), "already closed, so it starts at once");
            let mut tallest = rest;
            for _ in 0..40 {
                n.step(1.0 / 120.0);
                tallest = tallest.max(n.silhouette().h);
            }
            assert!((2.0..9.0).contains(&(tallest - rest)), "a swell, not a jump: {}", tallest - rest);
            settle(&mut n);
            assert!((n.silhouette().h - rest).abs() < 0.01 && !n.is_animating());
        }
    }

    #[test]
    fn open_panel_waits_or_forgets() {
        let mut n = island();
        n.open_page(Page::Tools);
        settle(&mut n);
        n.swallow();
        n.step(1.0 / 60.0);
        assert!(n.swallowing.is_some() && n.gulp.is_settled(), "nothing swells while the panel is open");
        n.collapse();
        settle(&mut n);
        assert!(n.swallowing.is_none(), "it swelled as the panel closed");
        n.open_page(Page::Tools);
        settle(&mut n);
        n.swallowing = Some(Instant::now());
        n.step(1.0 / 60.0);
        assert!(n.swallowing.is_none() && n.gulp.is_settled(), "and gives up when its time has passed");
    }
}
