//! The song playing now, on the closed notch: its cover, title and moving bars, tinted with a color from the
//! artwork. The files here cover when it shows, how it paints, the beat a press makes and the volume strip under it.

mod anticipate;
mod clock;
mod dance;
mod paint;
mod pulse;
mod reveal;
mod sample;
mod strip;
mod warp;

pub(super) use strip::Strip;
#[cfg(test)]
mod tests;

use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::{Color32, Painter, Rect, Vec2, pos2, vec2};

use crate::bridge::Page;
use crate::media::{BANDS, Bands, NowPlaying};
use crate::platform::Style;
use crate::ui::canvas::Canvas;
use crate::ui::color::{self, white};
use crate::ui::cover::{Art, Cover};
use crate::ui::handover::Handover;
use crate::ui::spring::smoothstep;

use super::geometry::Geometry;
use super::{Mode, Notch};
use anticipate::Expectation;
use pulse::{Pulse, beat_between, skipped};

const NOTCH_WING: f32 = 44.0;
const NOTCH_DROP: f32 = 24.0;
pub(super) const NOTCH_DEPTH: f32 = 8.0;
const ISLAND_W: f32 = 250.0;
pub(super) const ISLAND_H: f32 = 48.0;
pub(super) const COMPACT_H: f32 = 34.0;
pub(super) const ISLAND_DEPTH: f32 = 6.5;
pub(super) const DOCKED_DEPTH: f32 = 0.8;
const SHOWN: f32 = 0.001;
const TINT_FOR: Duration = Duration::from_millis(400);

/// What the notch knows about the song: the last report, its cover and tint, and any beat still playing out.
pub(super) struct Music {
    now: NowPlaying,
    cover: Cover,
    tint: Tint,
    second: Option<u64>,
    pulse: Option<Pulse>,
    skip_direction: f32,
    expecting: Option<Expectation>,
    words: Handover<NowPlaying>,
}

impl Music {
    fn new(now: NowPlaying) -> Self {
        let cover = Cover::new("now-playing", now.artwork.as_ref());
        let tint = Tint::steady(tint_of(cover.shown()), Instant::now());
        Self {
            now,
            cover,
            tint,
            second: None,
            pulse: None,
            skip_direction: 1.0,
            expecting: None,
            words: Handover::default(),
        }
    }

    fn change_to(&mut self, next: &NowPlaying, at: Instant) {
        if skipped(&self.now, next) {
            let direction = self.skip_direction;
            self.words.hand_over(self.now.clone(), direction, at);
            if self.cover.song_changed(next.artwork.as_ref(), direction, at) {
                self.retint(at);
            }
            self.pulse = None;
            self.skip_direction = 1.0;
        } else {
            if let Some(beat) = beat_between(&self.now, next) {
                self.pulse = Some(Pulse::new(beat, at));
                self.skip_direction = 1.0;
            }
            if !same_art(&self.now, next) && self.cover.art_changed(next.artwork.as_ref(), at) {
                self.retint(at);
            }
        }
        self.now = next.clone();
    }

    fn retint(&mut self, at: Instant) {
        self.tint.set(tint_of(self.cover.shown()), at);
    }

    fn remaining(&self, now: Instant) -> Option<f64> {
        self.now.remaining(now)
    }

    fn moves(&self, now: Instant) -> bool {
        self.words.is_moving(now) || self.cover.is_moving(now) || self.tint.is_moving(now)
    }
}

#[derive(Clone, Copy, Debug)]
struct Tint {
    from: Color32,
    to: Color32,
    at: Instant,
}

impl Tint {
    fn steady(color: Color32, at: Instant) -> Self {
        Self { from: color, to: color, at }
    }

    fn set(&mut self, color: Color32, at: Instant) {
        if color != self.to {
            *self = Self { from: self.at(at), to: color, at };
        }
    }

    fn at(&self, now: Instant) -> Color32 {
        let t = now.saturating_duration_since(self.at).as_secs_f32() / TINT_FOR.as_secs_f32();
        color::mix(self.from, self.to, smoothstep(0.0, 1.0, t))
    }

    fn is_moving(&self, now: Instant) -> bool {
        self.from != self.to && now < self.at + TINT_FOR
    }
}

fn same_art(a: &NowPlaying, b: &NowPlaying) -> bool {
    match (&a.artwork, &b.artwork) {
        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
        (None, None) => true,
        _ => false,
    }
}

fn tint_of(art: Option<&Art>) -> Color32 {
    art.and_then(|art| color::vibrant(&art.image.pixels)).unwrap_or_else(|| white(0.85))
}

impl Notch {
    pub fn set_now_playing(&mut self, now: Option<&NowPlaying>) -> bool {
        if self.preview.is_some() {
            return false;
        }
        let believed = match (&mut self.music, now) {
            (Some(music), Some(reported)) => Some(music.believed(reported, Instant::now())),
            (_, reported) => reported.cloned(),
        };
        let now = believed.as_ref();
        let noteworthy = match (&self.music, now) {
            (Some(music), Some(now)) => {
                music.now.title != now.title || music.now.artist != now.artist || music.now.playing != now.playing
            }
            (None, Some(_)) => true,
            (_, None) => false,
        };
        let mut changed = self.music.is_none() != now.is_none();
        let shown = self.music_alpha(self.morph.current()) > SHOWN;
        if now.is_some_and(|now| !now.playing) {
            self.liquid.set([0.0; BANDS]);
        }
        match (&mut self.music, now) {
            (Some(music), Some(now)) => {
                if !music.now.same_as(now) {
                    music.change_to(now, Instant::now());
                    changed = true;
                }
            }
            (music, now) => {
                let gone = std::mem::replace(music, now.cloned().map(Music::new));
                self.music_leaving = gone.filter(|_| shown && now.is_none());
            }
        }
        let leads = self.music_leads();
        self.arrange();
        if leads != self.music_leads() {
            self.retarget(super::moves::transition::Motion::Resize);
        }
        self.follow_reveal(noteworthy);
        changed
    }

    pub(super) fn music_playing(&self) -> bool {
        self.music.as_ref().is_some_and(|music| music.now.playing)
    }

    pub fn set_music_levels(&mut self, bands: Bands) {
        if self.preview.is_some() {
            return;
        }
        self.follow_dance();
        self.liquid.set(if self.dances() { bands } else { [0.0; BANDS] });
    }

    pub(super) fn music_geometry(&self) -> Geometry {
        match self.style {
            Style::Notch => {
                Geometry { w: self.notch.x + 2.0 * NOTCH_WING, h: self.notch.y + NOTCH_DROP, r: 14.0, ear: 6.0 }
            }
            Style::Island => {
                let head = if self.strip.is_some() { COMPACT_H } else { ISLAND_H };
                Geometry { w: ISLAND_W, h: head, r: ISLAND_H * 0.5, ear: 0.0 }
            }
        }
    }

    pub(super) fn shows_music(&self) -> bool {
        self.music_revealed()
            && self.live.is_none()
            && !self.activity.is_live()
            && matches!(self.mode, Mode::Idle | Mode::Peek)
    }

    pub(super) fn opens_to_music(&mut self) {
        let wanted =
            crate::config::current().music.open_to_player && self.tabs.iter().any(|tab| tab.page == Page::Media);
        if wanted && self.music.is_some() && matches!(self.mode, Mode::Idle | Mode::Peek) {
            self.pager.show(Page::Media);
        }
    }

    pub(super) fn music_is_moving(&self) -> bool {
        (self.music_revealed() || self.dance.value > 0.001) && self.liquid.is_moving()
    }

    pub(super) fn upload_artwork(&mut self, ctx: &egui::Context) {
        if let Some(music) = &mut self.music {
            music.cover.upload(ctx);
        }
    }

    pub(in crate::notch) fn let_the_song_go(&mut self) {
        if self.music.is_some() || self.music_fade.value <= SHOWN {
            self.music_leaving = None;
        }
    }

    pub(super) fn music_ui(&self, painter: &Painter, g: Geometry, body: Rect) {
        let alpha = self.music_alpha(g);
        let Some(music) = self.music.as_ref().or(self.music_leaving.as_ref()).filter(|_| alpha > SHOWN) else {
            return;
        };
        let mut canvas = Canvas::new(painter);
        let now = Instant::now();
        match self.style {
            Style::Notch => self.paint_notch_music(&mut canvas, music, body, now),
            Style::Island => self.paint_island_music(&mut canvas, music, body, now),
        }
        let clip = painter.with_clip_rect(body.expand2(vec2(1.0, 1.0 + NOTCH_DEPTH)));
        canvas.paint(&clip, pos2(body.center().x, body.top()), 1.0, Vec2::ZERO, alpha);
    }

    pub(super) fn music_alpha(&self, g: Geometry) -> f32 {
        let (idle, target, open) = (self.idle_geometry(), self.music_geometry(), self.expanded_geometry());
        let progress = |v: f32, from: f32, to: f32| if to - from > 1.0 { (v - from) / (to - from) } else { 0.0 };
        let grown = match self.style {
            Style::Notch => progress(g.w, idle.w, target.w).max(progress(g.h, idle.h, target.h)),
            Style::Island => 1.0,
        };
        let beyond = progress(g.h, self.peek_of(target).h, open.h);
        let attract = smoothstep(0.8, 1.0, self.attract.value);
        self.music_fade.value.clamp(0.0, 1.0)
            * smoothstep(0.35, 0.9, grown)
            * (1.0 - smoothstep(0.0, 0.3, beyond))
            * attract
    }
}
