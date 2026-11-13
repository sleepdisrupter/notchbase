//! The player: cover, title and artist, the scrubber and the transport buttons.

mod holds;
mod scrubber;
mod transport;

use std::sync::Arc;
use std::time::Instant;

use egui::epaint::RectShape;
use egui::{Align2, Color32, CornerRadius, Pos2, Rect, Stroke, Vec2, pos2, vec2};

use super::PageCtx;
use crate::bridge::Action;
use crate::media::{Control, NowPlaying};
use crate::ui::canvas::{Canvas, fade};
use crate::ui::color::{label, secondary_label, tertiary_label, white};
use crate::ui::cover::{Art, Cover};
use crate::ui::glyph::{self, Glyph};
use crate::ui::handover::{Handover, Part};
use crate::ui::{controls, font};
use holds::Holds;
use scrubber::Scrubber;

const ART: f32 = 56.0;
const ART_RADIUS: f32 = 10.0;
const TEXT_GAP: f32 = 12.0;
const TITLE_RISE: f32 = 9.0;
const SCRUBBER_TOP: f32 = ART + 12.0;
const TRANSPORT_Y: f32 = SCRUBBER_TOP + scrubber::HEIGHT + 24.0;
const PLAYING_H: f32 = TRANSPORT_Y + 22.0;
const UNTIMED: f32 = scrubber::HEIGHT + 12.0;
const NOTHING_H: f32 = 24.0;
const NOTHING_GLYPH: f32 = 14.0;
const NOTHING_GAP: f32 = 8.0;
const NOTHING: &str = "Nothing Playing";
const ART_NAME: &str = "media-page-art";
const ART_SETTLE: f32 = 0.05;
const WORDS_SHIFT: f32 = 10.0;
const FULL_UV: Rect = Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0));

pub struct PlayerPage {
    cover: Cover,
    song: Option<NowPlaying>,
    words: Handover<NowPlaying>,
    direction: f32,
    scrubber: Scrubber,
    holds: Holds,
}

impl Default for PlayerPage {
    fn default() -> Self {
        Self {
            cover: Cover::new(ART_NAME, None),
            song: None,
            words: Handover::default(),
            direction: 1.0,
            scrubber: Scrubber::default(),
            holds: Holds::default(),
        }
    }
}

impl PlayerPage {
    pub fn height(&self, now_playing: Option<&NowPlaying>) -> f32 {
        match now_playing {
            Some(now) if now.timeline.is_some() => PLAYING_H,
            Some(_) => PLAYING_H - UNTIMED,
            None => NOTHING_H,
        }
    }

    pub fn is_animating(&self) -> bool {
        let now = Instant::now();
        self.scrubber.is_dragging()
            || self.holds.is_holding()
            || self.words.is_moving(now)
            || self.cover.is_moving(now)
            || self.cover.awaits()
    }

    pub fn ui(&mut self, ctx: &mut PageCtx<'_>, now_playing: Option<&NowPlaying>) -> Vec<Action> {
        let Some(now) = now_playing else {
            self.song = None;
            self.cover = Cover::new(ART_NAME, None);
            paint_nothing(ctx.canvas, ctx.rect);
            return Vec::new();
        };
        self.follow(now, ctx.now);
        self.cover.upload(ctx.ui.ctx());
        let mut actions = Vec::new();
        let interactive = ctx.alpha >= 0.5;
        let playing = self.holds.playing(now.playing, ctx.now);
        let art = Rect::from_min_size(ctx.rect.min, Vec2::splat(ART));
        self.paint_cover(ctx.canvas, art, ctx.now);
        if let Some((song, part)) = self.words.leaving(ctx.now) {
            paint_titles(ctx.canvas, song, art, ctx.rect.right(), part);
        }
        paint_titles(ctx.canvas, now, art, ctx.rect.right(), self.words.arriving(ctx.now));

        if let Some(timeline) = now.timeline {
            let track = Rect::from_min_size(
                pos2(ctx.rect.left(), ctx.rect.top() + SCRUBBER_TOP),
                vec2(ctx.rect.width(), scrubber::HEIGHT),
            );
            let actual = timeline.position_at(ctx.now, now.playing);
            let shown = self.holds.position(actual, now.playing, ctx.now);
            if let Some(seconds) = self.scrubber.ui(ctx, track, shown, timeline.duration, interactive) {
                self.holds.hold_position(seconds, ctx.now);
                actions.push(Action::Media(Control::Seek(seconds)));
            }
        }

        let lift = if now.timeline.is_some() { 0.0 } else { UNTIMED };
        let center = pos2(ctx.rect.center().x, ctx.rect.top() + TRANSPORT_Y - lift);
        if let Some(control) = transport::ui(ctx, center, playing, interactive) {
            match control {
                Control::PlayPause => self.holds.hold_playing(!playing, ctx.now),
                Control::Next => self.direction = 1.0,
                Control::Previous => self.direction = -1.0,
                Control::Seek(_) => {}
            }
            actions.push(Action::Media(control));
        }
        actions
    }

    fn follow(&mut self, now: &NowPlaying, at: Instant) {
        match &self.song {
            None => self.cover = Cover::new(ART_NAME, now.artwork.as_ref()),
            Some(shown) if shown.title != now.title || shown.artist != now.artist => {
                self.words.hand_over(shown.clone(), self.direction, at);
                self.cover.song_changed(now.artwork.as_ref(), self.direction, at);
                self.direction = 1.0;
            }
            Some(shown) if !same_art(shown, now) => {
                self.cover.art_changed(now.artwork.as_ref(), at);
            }
            Some(_) => {}
        }
        self.cover.tick(at);
        self.song = Some(now.clone());
    }

    fn paint_cover(&self, canvas: &mut Canvas, rect: Rect, now: Instant) {
        let arriving = self.cover.arriving(now).alpha;
        if let Some((art, _)) = self.cover.leaving(now) {
            let covered = self.cover.shown().is_some_and(|art| art.texture.is_some());
            paint_art(canvas, art, rect, if covered { 1.0 } else { 1.0 - arriving });
        }
        paint_art(canvas, self.cover.shown(), rect, arriving);
    }
}

fn same_art(a: &NowPlaying, b: &NowPlaying) -> bool {
    match (&a.artwork, &b.artwork) {
        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
        (None, None) => true,
        _ => false,
    }
}

fn paint_art(canvas: &mut Canvas, art: Option<&Art>, frame: Rect, alpha: f32) {
    if alpha <= 0.01 {
        return;
    }
    let rect = Rect::from_center_size(frame.center(), frame.size() * (1.0 - ART_SETTLE * (1.0 - alpha)));
    match art.and_then(|art| art.texture.as_ref()) {
        Some(texture) => {
            let corner = CornerRadius::same(ART_RADIUS as u8);
            let lit = Color32::WHITE.gamma_multiply(alpha);
            canvas.add(RectShape::filled(rect, corner, lit).with_texture(texture.id(), FULL_UV));
            canvas.rect_stroke(rect, ART_RADIUS, Stroke::new(1.0, white(0.1 * alpha)));
        }
        None => {
            canvas.rect(rect, ART_RADIUS, white(0.1 * alpha));
            let glyph = tertiary_label().gamma_multiply(alpha);
            glyph::paint(canvas, Glyph::Note, rect.center(), ART * 0.42, glyph);
        }
    }
}

fn paint_titles(canvas: &mut Canvas, now: &NowPlaying, art: Rect, right: f32, part: Part) {
    if part.alpha <= 0.01 {
        return;
    }
    let left = art.right() + TEXT_GAP;
    let room = (right - left).max(0.0);
    let mark = canvas.mark();
    let title_font = font::semibold(14.0);
    let title = controls::truncate(canvas, &now.title, title_font.clone(), room);
    let lone = now.artist.is_empty();
    let title_y = art.center().y - if lone { 0.0 } else { TITLE_RISE };
    canvas.text(pos2(left, title_y), Align2::LEFT_CENTER, title, title_font, label());
    if !lone {
        let artist = controls::truncate(canvas, &now.artist, font::body(), room);
        let at = pos2(left, art.center().y + TITLE_RISE);
        canvas.text(at, Align2::LEFT_CENTER, artist, font::body(), secondary_label());
    }
    let shift = vec2(part.shift * WORDS_SHIFT, 0.0);
    for shape in canvas.since(mark) {
        shape.translate(shift);
        fade(shape, part.alpha);
    }
    if part.shift != 0.0 {
        canvas.clip(mark, Rect::from_x_y_ranges(left..=right, f32::MIN..=f32::MAX));
    }
}

fn paint_nothing(canvas: &mut Canvas, rect: Rect) {
    let well = Rect::from_min_size(rect.min, Vec2::splat(NOTHING_H));
    glyph::paint(canvas, Glyph::Note, well.center(), NOTHING_GLYPH, tertiary_label());
    let at = pos2(well.right() + NOTHING_GAP, well.center().y);
    canvas.text(at, Align2::LEFT_CENTER, NOTHING, font::headline(), secondary_label());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_row_when_nothing_plays() {
        let page = PlayerPage::default();
        let song =
            NowPlaying { title: "A".into(), artist: String::new(), artwork: None, playing: true, timeline: None };
        assert_eq!(page.height(None), NOTHING_H);
        assert_eq!(page.height(Some(&song)), PLAYING_H - UNTIMED);
        let timed = NowPlaying { timeline: crate::media::Timeline::new(1.0, 60.0, std::time::Instant::now()), ..song };
        assert_eq!(page.height(Some(&timed)), PLAYING_H);
        const { assert!(TRANSPORT_Y + 22.0 <= PLAYING_H) };
        assert!(!page.is_animating());
    }

    #[test]
    fn new_song_waits_for_cover() {
        let mut page = PlayerPage::default();
        let t0 = Instant::now();
        let art = Arc::new(egui::ColorImage::new([2, 2], vec![Color32::RED; 4]));
        let first = NowPlaying {
            title: "A".into(),
            artist: "B".into(),
            artwork: Some(art.clone()),
            playing: true,
            timeline: None,
        };
        page.follow(&first, t0);
        assert!(!page.is_animating());
        page.direction = -1.0;
        page.follow(&NowPlaying { title: "Z".into(), artwork: None, ..first }, t0);
        assert!(page.words.leaving(t0).is_some_and(|(song, part)| song.title == "A" && part.alpha > 0.99));
        assert!(page.words.arriving(t0).shift < -0.99, "it went back, so the new title comes from the left");
        let waits = page.cover.shown().is_some_and(|shown| Arc::ptr_eq(&shown.image, &art));
        assert!(waits && page.is_animating(), "the old cover stays until the next one arrives");
        assert_eq!(page.direction, 1.0);
    }
}
