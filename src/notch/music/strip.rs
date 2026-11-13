//! The strip that grows under a playing song to show a volume or brightness change.

use std::time::Instant;

use egui::Rect;

use crate::notch::activity::volume::{Indicator, IndicatorRow, paint_indicator_row};
use crate::notch::moves::transition::Motion;
use crate::ui::canvas::{Canvas, fade};
use crate::ui::motion;
use crate::ui::spring::smoothstep;

use super::super::{Mode, Notch};

pub(in crate::notch) const STRIP_H: f32 = 20.0;
const STRIP_GLYPH: f32 = 12.0;

pub(super) struct Lane {
    pub(super) left: f32,
    pub(super) column: f32,
    pub(super) right: f32,
    pub(super) y: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::notch) struct Strip {
    indicator: Indicator,
    until: Instant,
}

impl Strip {
    pub(in crate::notch) fn gone(self) -> Indicator {
        self.indicator
    }
}

impl Notch {
    pub(in crate::notch) fn carries_indicator(&self) -> bool {
        self.music_playing() && crate::config::current().music.in_notch
    }

    pub(in crate::notch) fn show_indicator_on_music(&mut self, indicator: Indicator, now: Instant) {
        if self.activity.is_live() {
            self.activity.until = None;
            if self.mode == Mode::Activity {
                self.set_mode(Mode::Idle, Motion::Pop);
            }
        }
        let fresh = self.strip.is_none();
        self.strip_gone = None;
        if self.strip.is_some_and(|strip| indicator.continues(strip.indicator)) {
            self.level_meter.follow(indicator.level());
        } else {
            self.level_meter.snap(indicator.level());
        }
        let until = now + indicator.duration();
        self.strip = Some(Strip { indicator, until });
        self.compact.animate_to(1.0, motion::HOVER);
        self.reveal_music_until(until);
        if fresh {
            self.retarget(if self.mode == Mode::Peek { Motion::Peek } else { Motion::Pop });
        }
    }

    pub(in crate::notch) fn pin_sample_strip(&mut self, volume: f32, until: Instant) {
        self.level_meter.snap(volume);
        self.strip = Some(Strip { indicator: Indicator::Volume { volume, muted: false }, until });
        self.compact.snap_to(1.0);
    }

    pub(in crate::notch) fn expire_strip(&mut self, now: Instant) -> bool {
        if self.strip.is_none_or(|strip| now < strip.until) {
            return false;
        }
        self.strip_gone = self.strip.take().map(Strip::gone);
        self.compact.animate_to(0.0, motion::HOVER);
        self.conceal_music();
        self.retarget(if self.mode == Mode::Peek { Motion::Peek } else { Motion::Close });
        true
    }

    pub(in crate::notch) fn strip_height(&self) -> f32 {
        if self.strip.is_some() { STRIP_H } else { 0.0 }
    }

    pub(in crate::notch) fn compactness(&self) -> f32 {
        self.compact.value.clamp(0.0, 1.0)
    }

    pub(in crate::notch) fn strip_wakeup(&self) -> Option<Instant> {
        self.strip.map(|strip| strip.until)
    }

    pub(in crate::notch) fn strip_is_moving(&self) -> bool {
        self.strip.is_some() && self.shows_music() && self.level_meter.is_moving()
    }

    pub(in crate::notch) fn let_the_strip_go(&mut self) {
        if self.strip_gone.is_some() && self.morph.h.is_settled() {
            self.strip_gone = None;
        }
    }

    pub(super) fn paint_strip(&self, canvas: &mut Canvas, body: Rect, top: f32, lane: &Lane) {
        let Some(indicator) = self.strip.map(|strip| strip.indicator).or(self.strip_gone) else { return };
        let shown = smoothstep(0.35, 1.0, (body.bottom() - top) / STRIP_H);
        if shown <= 0.01 {
            return;
        }
        let row =
            IndicatorRow { left: lane.left, right: lane.right, y: lane.y, glyph: STRIP_GLYPH, column: lane.column };
        let mark = canvas.mark();
        paint_indicator_row(canvas, &row, self.level_meter.reading(), indicator);
        for shape in canvas.since(mark) {
            fade(shape, shown);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notch::music::ISLAND_H;
    use crate::notch::tests::fixtures::{AWAY, island, settle, song};

    #[test]
    fn volume_grows_strip_under_song() {
        let mut n = island();
        let t0 = Instant::now();
        n.set_now_playing(Some(&song(true, 10.0)));
        n.tick(t0, Some(AWAY));
        settle(&mut n);
        assert_eq!(n.hug.value, 1.0, "docked while nothing is happening");

        n.show_volume(0.4, false);
        assert!(!n.activity.is_live(), "no separate volume bubble");
        assert!(n.strip.is_some() && n.shows_music());
        n.tick(t0, Some(AWAY));
        settle(&mut n);
        assert_eq!(n.hug.value, 0.0, "the full player comes out to hold the bar");
        let tall = n.music_geometry();
        assert_eq!(n.silhouette().rect(), n.body_rect(tall));

        let until = n.strip.map(|strip| strip.until).expect("strip");
        n.show_volume(0.42, false);
        assert!(n.strip.map(|strip| strip.until).expect("strip") >= until);

        let closes_at = n.strip.map(|strip| strip.until).expect("strip");
        n.tick(closes_at, Some(AWAY));
        assert!(n.strip.is_none());
        assert!(!n.shows_music(), "the song closes with the volume bar instead of leaving a pill behind");
        assert_eq!(n.music_geometry().h, ISLAND_H);
        assert!(tall.h - STRIP_H < ISLAND_H, "the song row tucks into one line while the bar shows");
        settle(&mut n);
        assert!(n.is_collapsed());
    }

    #[test]
    fn brightness_shares_strip() {
        let mut n = island();
        n.set_now_playing(Some(&song(true, 10.0)));
        n.show_volume(0.4, false);
        n.show_brightness(0.75);
        assert!(!n.activity.is_live(), "no separate brightness bubble");
        assert_eq!(n.strip.map(|strip| strip.indicator), Some(Indicator::Brightness(0.75)));
        assert_eq!(n.level_meter.reading().level, 0.75, "the meter jumps between volume and brightness");
    }

    #[test]
    fn volume_alone_without_music() {
        let mut n = island();
        n.show_volume(0.4, false);
        assert!(n.activity.is_live() && n.strip.is_none());
        n.set_now_playing(Some(&song(false, 10.0)));
        assert!(!n.carries_indicator(), "a paused song doesn't carry the bar");
    }

    #[test]
    fn song_carries_bar_beside_activities() {
        let mut n = island();
        let t0 = Instant::now();
        n.show_volume(0.3, false);
        assert!(n.activity.is_live());
        n.set_activities(vec![crate::notch::live::tests::claude()]);
        n.set_now_playing(Some(&song(true, 10.0)));
        n.show_volume(0.4, false);
        assert!(!n.activity.is_live(), "the lone volume bubble gives way to the song");
        assert!(n.strip.is_some() && n.shows_music(), "the song shows with the bar while Claude works");
        n.tick(t0, Some(AWAY));
        settle(&mut n);
        assert_eq!(n.silhouette().rect(), n.body_rect(n.music_geometry()));
    }

    #[test]
    fn expired_strip_folds_away() {
        let mut n = island();
        let t0 = Instant::now();
        n.set_now_playing(Some(&song(true, 10.0)));
        n.show_volume(0.4, false);
        settle(&mut n);
        let until = n.strip_wakeup().expect("a strip");
        assert!(n.expire_strip(until));
        assert!(n.strip.is_none() && n.strip_gone.is_some(), "it stays drawn while the shape folds");
        assert_eq!(n.strip_height(), 0.0, "and the shape no longer makes room for it");
        n.step(1.0 / 60.0);
        assert!(n.strip_gone.is_some());
        settle(&mut n);
        assert!(n.strip_gone.is_none(), "it is let go once the fold is done");
        n.show_volume(0.6, false);
        assert!(n.strip_gone.is_none());
        let _ = t0;
    }
}
