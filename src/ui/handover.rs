//! Hands one view over to the next in 450 ms. The old one slides out and fades, then the new one slides in after it.

use std::time::{Duration, Instant};

use super::spring::smoothstep;

const TAKES: Duration = Duration::from_millis(450);
const GONE_BY: f32 = 0.4;
const SHOWN_FROM: f32 = 0.25;
const SHOWN_BY: f32 = 0.85;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Part {
    pub alpha: f32,
    pub shift: f32,
}

impl Part {
    pub const RESTING: Self = Self { alpha: 1.0, shift: 0.0 };
}

pub struct Handover<T> {
    leaving: Option<(T, Instant, f32)>,
}

impl<T> Default for Handover<T> {
    fn default() -> Self {
        Self { leaving: None }
    }
}

impl<T> Handover<T> {
    /// If the old view from the last change still shows more than the new one, it stays the one leaving.
    pub fn hand_over(&mut self, previous: T, direction: f32, now: Instant) {
        let keeps = self.leaving.as_ref().is_some_and(|(_, at, _)| {
            let p = progress(*at, now);
            p < 1.0 && outgoing(p) > incoming(p)
        });
        if !keeps {
            self.leaving = Some((previous, now, direction));
        }
    }

    pub fn leaving(&self, now: Instant) -> Option<(&T, Part)> {
        let (previous, at, direction) = self.leaving.as_ref()?;
        let p = progress(*at, now);
        (p < 1.0).then(|| (previous, Part { alpha: outgoing(p), shift: -direction * ease_out(p) }))
    }

    pub fn arriving(&self, now: Instant) -> Part {
        let Some((_, at, direction)) = &self.leaving else { return Part::RESTING };
        let p = progress(*at, now);
        if p >= 1.0 {
            return Part::RESTING;
        }
        Part { alpha: incoming(p), shift: direction * (1.0 - ease_out(p)) }
    }

    pub fn is_moving(&self, now: Instant) -> bool {
        self.leaving.as_ref().is_some_and(|(_, at, _)| progress(*at, now) < 1.0)
    }
}

fn progress(at: Instant, now: Instant) -> f32 {
    now.saturating_duration_since(at).as_secs_f32() / TAKES.as_secs_f32()
}

fn outgoing(p: f32) -> f32 {
    1.0 - smoothstep(0.0, GONE_BY, p)
}

fn incoming(p: f32) -> f32 {
    smoothstep(SHOWN_FROM, SHOWN_BY, p)
}

fn ease_out(p: f32) -> f32 {
    1.0 - (1.0 - p.clamp(0.0, 1.0)).powi(3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_slides_out_before_new_shows() {
        let t0 = Instant::now();
        let mut handover = Handover::default();
        assert_eq!(handover.arriving(t0), Part::RESTING);
        handover.hand_over("old", 1.0, t0);
        let (old, part) = handover.leaving(t0).expect("leaving");
        assert_eq!((*old, part), ("old", Part { alpha: 1.0, shift: 0.0 }));
        assert_eq!(handover.arriving(t0), Part { alpha: 0.0, shift: 1.0 }, "the new one waits on its side");
        let middle = t0 + TAKES / 2;
        let (_, out) = handover.leaving(middle).expect("still leaving");
        assert!(out.alpha < 0.01 && out.shift < -0.8, "{out:?}");
        assert!(handover.arriving(middle).alpha > 0.3);
        assert!(handover.is_moving(middle));
        assert!(handover.leaving(t0 + TAKES).is_none());
        assert_eq!(handover.arriving(t0 + TAKES), Part::RESTING);
        assert!(!handover.is_moving(t0 + TAKES));
    }

    #[test]
    fn going_back_slides_the_other_way() {
        let t0 = Instant::now();
        let mut handover = Handover::default();
        handover.hand_over((), -1.0, t0);
        assert!(handover.arriving(t0).shift < -0.99);
        assert!(handover.leaving(t0 + TAKES / 4).is_some_and(|(_, part)| part.shift > 0.0));
    }

    #[test]
    fn quick_change_keeps_the_shown_one() {
        let t0 = Instant::now();
        let mut handover = Handover::default();
        handover.hand_over("first", 1.0, t0);
        let soon = t0 + TAKES / 20;
        handover.hand_over("second", 1.0, soon);
        assert_eq!(handover.leaving(soon).map(|(name, _)| *name), Some("first"), "the second was never seen");
        assert!(handover.arriving(soon).alpha < 0.01, "and the third takes its place on the way in");
        let later = t0 + TAKES * 3 / 4;
        handover.hand_over("third", 1.0, later);
        assert_eq!(handover.leaving(later).map(|(name, _)| *name), Some("third"));
        assert_eq!(handover.arriving(later).alpha, 0.0);
    }
}
