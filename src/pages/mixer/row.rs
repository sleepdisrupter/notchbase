//! A row's animated values. A volume you set holds until the system confirms it.

use std::time::{Duration, Instant};

use crate::audio::{AppId, AudioApp};
use crate::bridge::Action;
use crate::ui::canvas::Canvas;
use crate::ui::meter::Meter;
use crate::ui::spring::Spring;
use crate::ui::{controls, font, motion};

use super::thin_slider::KNOB_R;

const HOLD_FOR: Duration = Duration::from_millis(500);
const RESEND_EPSILON: f32 = 1e-4;
const CONFIRM_EPSILON: f32 = 0.005;

pub(super) struct RowState {
    pub(super) volume: Spring,
    pub(super) mute: Spring,
    pub(super) knob: Spring,
    pub(super) y: Spring,
    pub(super) appear: Spring,
    pub(super) meter: Meter,
    hold: Option<(f32, Instant)>,
    mute_hold: Option<(bool, Instant)>,
    pub(super) last_sent: Option<f32>,
    name: Option<FittedName>,
    pub(super) last: Option<AudioApp>,
    pub(super) alive: bool,
}

impl RowState {
    pub(super) fn new(app: Option<&AudioApp>, slot: f32, appear: bool) -> Self {
        let (volume, muted) = app.map_or((0.0, false), |a| (a.volume, a.muted));
        Self {
            volume: Spring::new(volume, motion::SMOOTH, 0.001),
            mute: Spring::new(muted as u8 as f32, motion::SMOOTH, 0.005),
            knob: Spring::new(KNOB_R, motion::PRESS, 0.01),
            y: Spring::new(slot, motion::SMOOTH, 0.1),
            appear: Spring::new(if appear { 0.0 } else { 1.0 }, motion::SMOOTH, 0.005),
            meter: Meter::default(),
            hold: None,
            mute_hold: None,
            last_sent: None,
            name: None,
            last: app.cloned(),
            alive: true,
        }
    }

    pub(super) fn step(&mut self, dt: f32) -> bool {
        let mut animating = false;
        for spring in [&mut self.volume, &mut self.mute, &mut self.knob, &mut self.y, &mut self.appear] {
            spring.step(dt);
            animating |= !spring.is_settled();
        }
        animating |= self.meter.step(dt);
        animating || self.hold.is_some() || self.mute_hold.is_some()
    }

    pub(super) fn shown_volume(&mut self, system: f32, now: Instant) -> f32 {
        if let Some((held, until)) = self.hold {
            if (system - held).abs() > CONFIRM_EPSILON && now < until {
                return held;
            }
            self.hold = None;
        }
        system
    }

    pub(super) fn shown_muted(&mut self, system: bool, now: Instant) -> bool {
        if let Some((held, until)) = self.mute_hold {
            if system != held && now < until {
                return held;
            }
            self.mute_hold = None;
        }
        system
    }

    pub(super) fn send_volume(&mut self, id: AppId, volume: f32, muted: bool, now: Instant, actions: &mut Vec<Action>) {
        if self.hold_volume(volume, now) {
            actions.push(Action::SetVolume(id, volume));
            if muted {
                self.send_muted(id, false, now, actions);
            }
        }
    }

    pub(super) fn send_muted(&mut self, id: AppId, muted: bool, now: Instant, actions: &mut Vec<Action>) {
        self.mute_hold = Some((muted, now + HOLD_FOR));
        actions.push(Action::SetMuted(id, muted));
    }

    pub(super) fn fitted_name(&mut self, canvas: &Canvas, name: &str, width: f32) -> String {
        let pixels_per_point = canvas.pixels_per_point();
        if let Some(fitted) = self.name.as_ref().filter(|f| f.fits(name, width, pixels_per_point)) {
            return fitted.text.clone();
        }
        let text = controls::truncate(canvas, name, font::body(), width);
        self.name = Some(FittedName { source: name.to_owned(), width, pixels_per_point, text: text.clone() });
        text
    }

    fn hold_volume(&mut self, volume: f32, now: Instant) -> bool {
        self.hold = Some((volume, now + HOLD_FOR));
        let send = self.last_sent.is_none_or(|sent| (sent - volume).abs() > RESEND_EPSILON);
        self.last_sent = Some(volume);
        send
    }
}

struct FittedName {
    source: String,
    width: f32,
    pixels_per_point: f32,
    text: String,
}

impl FittedName {
    fn fits(&self, name: &str, width: f32, pixels_per_point: f32) -> bool {
        self.source == name && self.width == width && self.pixels_per_point == pixels_per_point
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn held_values_win_until_confirmed() {
        let t0 = Instant::now();
        let mut row = RowState::new(None, 0.0, false);
        assert!(row.hold_volume(0.3, t0));
        assert!(!row.hold_volume(0.3, t0), "same value isn't resent");
        assert_eq!(row.shown_volume(0.8, t0 + Duration::from_millis(100)), 0.3);
        assert_eq!(row.shown_volume(0.301, t0 + Duration::from_millis(150)), 0.301);
        assert_eq!(row.shown_volume(0.5, t0 + Duration::from_millis(160)), 0.5);
        row.hold_volume(0.1, t0);
        assert_eq!(row.shown_volume(0.9, t0 + HOLD_FOR + Duration::from_millis(1)), 0.9);

        row.mute_hold = Some((true, t0 + HOLD_FOR));
        assert!(row.shown_muted(false, t0));
        assert!(!row.shown_muted(false, t0 + HOLD_FOR));
    }

    #[test]
    fn changing_volume_unmutes_once() {
        let t0 = Instant::now();
        let mut row = RowState::new(None, 0.0, false);
        let mut actions = Vec::new();
        row.send_volume(7, 0.4, true, t0, &mut actions);
        row.send_volume(7, 0.4, true, t0, &mut actions);
        assert_eq!(actions, vec![Action::SetVolume(7, 0.4), Action::SetMuted(7, false)], "resending is skipped");
        assert!(!row.shown_muted(true, t0));
    }

    #[test]
    fn names_refit_on_change() {
        let fitted = FittedName { source: "Music".into(), width: 118.0, pixels_per_point: 2.0, text: "Music".into() };
        assert!(fitted.fits("Music", 118.0, 2.0));
        assert!(!fitted.fits("Podcasts", 118.0, 2.0));
        assert!(!fitted.fits("Music", 90.0, 2.0));
        assert!(!fitted.fits("Music", 118.0, 1.5));
    }
}
