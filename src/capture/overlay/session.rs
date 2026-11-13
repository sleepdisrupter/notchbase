//! The life of a capture overlay. Opening fades it in. Closing holds it, fades it out, and only then hands back the
//! outcome, so the app acts once the overlay is gone.

use std::time::Instant;

use super::clock::elapsed;
use crate::ui::spring::smoothstep;

/// How a session closes: it holds for `hold` seconds, then fades out until `end`.
#[derive(Clone, Copy)]
pub(in crate::capture) struct FadeOut {
    pub(in crate::capture) hold: f32,
    pub(in crate::capture) end: f32,
}

enum Stage<E> {
    Idle,
    Open { since: Instant },
    Closing { since: Instant, from: f32, fade: FadeOut, exit: E },
}

pub(in crate::capture) struct Session<E> {
    stage: Stage<E>,
    fade_in: f32,
}

impl<E> Session<E> {
    pub(in crate::capture) fn new(fade_in: f32) -> Self {
        Self { stage: Stage::Idle, fade_in }
    }

    pub(in crate::capture) fn is_idle(&self) -> bool {
        matches!(self.stage, Stage::Idle)
    }

    pub(in crate::capture) fn is_open(&self) -> bool {
        matches!(self.stage, Stage::Open { .. })
    }

    pub(in crate::capture) fn open(&mut self, now: Instant) {
        self.stage = Stage::Open { since: now };
    }

    pub(in crate::capture) fn close(&mut self, now: Instant, exit: E, fade: FadeOut) {
        let from = self.opacity(now);
        self.stage = Stage::Closing { since: now, from, fade, exit };
    }

    pub(in crate::capture) fn opacity(&self, now: Instant) -> f32 {
        match &self.stage {
            Stage::Idle => 0.0,
            Stage::Open { since } => {
                let t = (elapsed(*since, now) / self.fade_in).clamp(0.0, 1.0);
                1.0 - (1.0 - t) * (1.0 - t)
            }
            Stage::Closing { since, from, fade, .. } => {
                from * (1.0 - smoothstep(fade.hold, fade.end, elapsed(*since, now)))
            }
        }
    }

    pub(in crate::capture) fn is_fading(&self, now: Instant) -> bool {
        match &self.stage {
            Stage::Open { since } => elapsed(*since, now) < self.fade_in,
            _ => self.closing(now).is_some(),
        }
    }

    pub(in crate::capture) fn closing(&self, now: Instant) -> Option<(&E, f32)> {
        match &self.stage {
            Stage::Closing { since, exit, .. } => Some((exit, elapsed(*since, now))),
            _ => None,
        }
    }

    pub(in crate::capture) fn take_exit(&mut self, now: Instant) -> Option<E> {
        match std::mem::replace(&mut self.stage, Stage::Idle) {
            Stage::Closing { since, fade, exit, .. } if elapsed(since, now) >= fade.end => Some(exit),
            stage => {
                self.stage = stage;
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    const FADE: FadeOut = FadeOut { hold: 0.04, end: 0.16 };

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn session_fades_in_and_stays_open() {
        let start = Instant::now();
        let mut session = Session::<u8>::new(0.12);
        assert!(session.is_idle() && !session.is_open());
        assert_eq!(session.opacity(start), 0.0);
        session.open(start);
        assert!(session.is_open() && !session.is_idle());
        assert_eq!(session.opacity(start), 0.0);
        assert!(session.is_fading(start + ms(60)));
        assert!(session.opacity(start + ms(60)) > 0.5);
        assert!(!session.is_fading(start + ms(120)));
        assert_eq!(session.opacity(start + ms(120)), 1.0);
        assert!(session.closing(start).is_none());
        assert_eq!(session.take_exit(start + ms(500)), None);
        assert!(session.is_open());
    }

    #[test]
    fn closing_holds_fades_then_exits() {
        let start = Instant::now();
        let mut session = Session::new(0.12);
        session.open(start);
        let closed = start + ms(60);
        let from = session.opacity(closed);
        session.close(closed, 7u8, FADE);
        assert!(!session.is_open() && !session.is_idle());
        assert!(session.is_fading(closed));
        assert_eq!(session.opacity(closed + ms(40)), from);
        assert!(session.opacity(closed + ms(100)) < from);
        let (exit, since) = session.closing(closed + ms(20)).unwrap();
        assert_eq!((*exit, (since * 1000.0).round()), (7, 20.0));
        assert_eq!(session.take_exit(closed + ms(150)), None);
        assert_eq!(session.take_exit(closed + ms(160)), Some(7));
        assert!(session.is_idle());
        assert_eq!(session.take_exit(closed + ms(200)), None);
    }
}
