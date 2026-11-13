//! Fades from one state to the next. The status line and the button row use it.

use crate::ui::motion;
use crate::ui::spring::{Spring, UNIT_PRECISION, smoothstep};

const GONE_BY: f32 = 0.5;
const SHOWN_FROM: f32 = 0.25;

pub(super) struct Crossfade<T> {
    shown: Option<T>,
    previous: Option<T>,
    started: bool,
    fade: Spring,
}

impl<T: Clone + PartialEq> Crossfade<T> {
    pub(super) fn new() -> Self {
        Self { shown: None, previous: None, started: false, fade: Spring::new(1.0, motion::SMOOTH, UNIT_PRECISION) }
    }

    pub(super) fn show(&mut self, state: Option<&T>, dt: f32) -> f32 {
        if self.shown.as_ref() != state {
            let showing_more = incoming(self.fade.value) >= outgoing(self.fade.value);
            match self.shown.take() {
                Some(current) if showing_more || self.previous.is_none() => {
                    self.previous = Some(current);
                    self.fade.snap_to(0.0);
                }
                Some(_) => {}
                None if self.previous.is_none() && self.started => self.fade.snap_to(0.0),
                None => {}
            }
            self.fade.animate_to(1.0, motion::SMOOTH);
            self.shown = state.cloned();
            self.started |= state.is_some();
        }
        self.fade.step(dt);
        if self.fade.is_settled() {
            self.previous = None;
        }
        incoming(self.fade.value)
    }

    pub(super) fn previous(&self) -> Option<(T, f32)> {
        self.previous.clone().map(|previous| (previous, outgoing(self.fade.value)))
    }

    pub(super) fn is_settled(&self) -> bool {
        self.fade.is_settled()
    }
}

fn incoming(progress: f32) -> f32 {
    smoothstep(SHOWN_FROM, 1.0, progress)
}

fn outgoing(progress: f32) -> f32 {
    1.0 - smoothstep(0.0, GONE_BY, progress)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_fades_in_old_fades_out() {
        let mut fade = Crossfade::new();
        assert_eq!(fade.show(Some(&1), 0.0), 1.0, "the first state is simply there");
        assert!(fade.previous().is_none());
        assert!(fade.show(Some(&2), 0.0) < 0.01);
        assert_eq!(fade.previous(), Some((1, 1.0)), "the old one leaves from where it was");
        let mut shown = 0.0;
        for _ in 0..120 {
            shown = fade.show(Some(&2), 1.0 / 60.0);
        }
        assert!(shown > 0.99 && fade.is_settled() && fade.previous().is_none());
    }

    #[test]
    fn quick_change_keeps_shown() {
        let mut fade = Crossfade::new();
        fade.show(Some(&1), 0.0);
        fade.show(Some(&2), 0.0);
        fade.show(Some(&2), 1.0 / 60.0);
        fade.show(Some(&3), 0.0);
        assert_eq!(fade.previous().map(|(state, _)| state), Some(1), "two was never really seen");
    }

    #[test]
    fn nothing_fades_out_and_back() {
        let mut fade = Crossfade::new();
        fade.show(Some(&1), 0.0);
        assert_eq!(fade.show(None, 0.0), 0.0);
        assert_eq!(fade.previous(), Some((1, 1.0)));
        for _ in 0..120 {
            fade.show(None, 1.0 / 60.0);
        }
        assert!(fade.previous().is_none());
        assert!(fade.show(Some(&2), 0.0) < 0.01, "after it was empty, the next one fades in");
    }
}
