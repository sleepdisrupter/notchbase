//! Paints the song: cover, bars, title and time left, laid out for a notch or for an island.

use std::time::{Duration, Instant};

use egui::epaint::RectShape;
use egui::{Align2, Color32, CornerRadius, Pos2, Rect, Stroke, Vec2, pos2, vec2};
use notchbase_sdk::clock;

use crate::media::NowPlaying;
use crate::notch::Notch;
use crate::notch::moves::hug::DOCK_H;
use crate::ui::canvas::{Canvas, Mark, fade};
use crate::ui::color::{self, label, secondary_label, white};
use crate::ui::cover::Art;
use crate::ui::glyph::{self, Glyph};
use crate::ui::handover::Part;
use crate::ui::spring::{lerp, smoothstep};
use crate::ui::{controls, font};

use super::pulse::{ArtLook, look};
use super::strip::Lane;
use super::warp::{Warp, paint_warped};
use super::{COMPACT_H, ISLAND_H, Music, NOTCH_DROP};

const NOTCH_ART: f32 = 20.0;
const NOTCH_INSET: f32 = 12.0;
const TUCKED_ART: f32 = 18.0;
const TUCKED_INSET: f32 = 9.0;
const TEXT_GAP: f32 = 10.0;
const ARTIST_GAP: f32 = 6.0;
const BARS: [usize; 4] = [0, 1, 3, 5];
const BAR_W: f32 = 3.0;
const BAR_GAP: f32 = 2.2;
const BAR_REST: f32 = 3.0;
const LINE_SPREAD: f32 = 7.5;
const COMPACT_SIDE: f32 = 15.0;
const SCRIM: f32 = 0.3;
const WARP_VISIBLE: f32 = 0.01;
const COVER_SETTLE: f32 = 0.06;
const WORDS_SHIFT: f32 = 8.0;
const FULL_UV: Rect = Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0));

struct Bars {
    right: f32,
    center_y: f32,
    tallest: f32,
}

struct IslandWords {
    left: f32,
    title_right: f32,
    time_right: f32,
    upper: f32,
    lower: f32,
    open: f32,
    reveal: f32,
}

struct NotchWords {
    left: f32,
    right: f32,
    y: f32,
    reveal: f32,
    detail: f32,
}

impl Notch {
    pub(super) fn paint_notch_music(&self, canvas: &mut Canvas, music: &Music, body: Rect, now: Instant) {
        let tuck = self.hug.value.clamp(0.0, 1.0);
        let lane_y = body.top() + self.notch.y * 0.5;
        let (inset, side) = (lerp(NOTCH_INSET, TUCKED_INSET, tuck), lerp(NOTCH_ART, TUCKED_ART, tuck));
        let home = Rect::from_center_size(pos2(body.left() + inset + side * 0.5, lane_y), Vec2::splat(side));
        let art = home.translate(vec2(self.lead(home.left()) - home.left(), 0.0));
        let tint = music.tint.at(now);
        paint_cover(canvas, music, art, 5.0, tint, now);
        let bars = Bars { right: body.right() - inset, center_y: lane_y, tallest: lerp(12.0, 11.0, tuck) };
        self.paint_bars(canvas, &bars, tint);
        let reveal = 1.0 - smoothstep(0.0, 0.6, tuck);
        if reveal <= 0.01 {
            return;
        }
        let title_y = body.top() + self.notch.y + NOTCH_DROP * 0.5 - 1.0;
        let detail = reveal * (1.0 - self.compactness());
        let words = NotchWords { left: home.left(), right: bars.right, y: title_y, reveal, detail };
        let mark = canvas.mark();
        if let Some((song, part)) = music.words.leaving(now) {
            paint_notch_words(canvas, song, &words, part, now);
        }
        paint_notch_words(canvas, &music.now, &words, music.words.arriving(now), now);
        fade_since(canvas, mark, self.song_words_shown());
        let lane = Lane { left: home.left(), right: bars.right, y: title_y };
        self.paint_strip(canvas, &lane, reveal);
    }

    pub(super) fn paint_island_music(&self, canvas: &mut Canvas, music: &Music, full: Rect, now: Instant) {
        let compact = self.compactness();
        let head = lerp(ISLAND_H, COMPACT_H, compact);
        let body = Rect::from_min_size(full.min, vec2(full.width(), full.height().min(head)));
        let open = ((body.height() - DOCK_H) / (ISLAND_H - DOCK_H)).clamp(0.0, 1.0);
        let center_y = body.center().y;
        let art_side = lerp(16.0, 30.0, open);
        let art_inset = lerp(lerp(7.0, 9.0, open), COMPACT_SIDE, compact);
        let art =
            Rect::from_center_size(pos2(body.left() + art_inset + art_side * 0.5, center_y), Vec2::splat(art_side));
        let corner = full.height().min(ISLAND_H) * 0.5;
        let radius = (corner - art_inset).clamp(lerp(5.0, 8.0, open), art_side * 0.5);
        let tint = music.tint.at(now);
        paint_cover(canvas, music, art, radius, tint, now);
        let upper = center_y - LINE_SPREAD * open * (1.0 - compact);
        let lower = center_y + 8.0;
        let resting = lerp(body.height() * 0.5, body.height() * 0.5 - 4.0, open);
        let bars = Bars {
            right: body.right() - lerp(resting, art_inset, compact),
            center_y: upper,
            tallest: lerp(9.0, 12.0, open),
        };
        let words_shown = self.song_words_shown();
        let mark = canvas.mark();
        let bars_left = self.paint_bars(canvas, &bars, tint);
        let words = IslandWords {
            left: art.right() + TEXT_GAP,
            title_right: bars_left - TEXT_GAP,
            time_right: bars.right,
            upper,
            lower,
            open,
            reveal: smoothstep(0.55, 1.0, open) * (1.0 - compact),
        };
        if let Some((song, part)) = music.words.leaving(now) {
            paint_island_words(canvas, song, &words, part, now);
        }
        paint_island_words(canvas, &music.now, &words, music.words.arriving(now), now);
        fade_since(canvas, mark, words_shown);
        let lane = Lane { left: art.right() + TEXT_GAP, right: bars.right, y: center_y };
        self.paint_strip(canvas, &lane, 1.0);
    }

    fn paint_bars(&self, canvas: &mut Canvas, bars: &Bars, tint: Color32) -> f32 {
        let width = BARS.len() as f32 * BAR_W + (BARS.len() - 1) as f32 * BAR_GAP;
        let left = bars.right - width;
        for (i, band) in BARS.into_iter().enumerate() {
            let level = self.liquid.level(band);
            let h = BAR_REST + (bars.tallest - BAR_REST) * level;
            let x = left + i as f32 * (BAR_W + BAR_GAP) + BAR_W * 0.5;
            let bar = Rect::from_center_size(pos2(x, bars.center_y), vec2(BAR_W, h));
            canvas.rect(bar, BAR_W * 0.5, tint);
        }
        left
    }
}

pub(super) fn remaining_label(seconds: f64) -> String {
    format!("-{}", clock(Duration::from_secs_f64(seconds.max(0.0).ceil())))
}

fn paint_remaining(canvas: &mut Canvas, song: &NowPlaying, right_center: Pos2, now: Instant, reveal: f32) -> f32 {
    let Some(left) = song.remaining(now).filter(|_| reveal > 0.01) else { return right_center.x };
    let tint = secondary_label().gamma_multiply(reveal);
    canvas.text(right_center, Align2::RIGHT_CENTER, remaining_label(left), font::mono(11.0), tint).left()
}

fn paint_island_words(canvas: &mut Canvas, song: &NowPlaying, at: &IslandWords, part: Part, now: Instant) {
    if part.alpha <= 0.01 {
        return;
    }
    let mark = canvas.mark();
    let remaining_left = paint_remaining(canvas, song, pos2(at.time_right, at.lower), now, at.reveal);
    let title_font = font::semibold(lerp(11.5, 12.5, at.open));
    let title = controls::truncate(canvas, &song.title, title_font.clone(), at.title_right - at.left);
    canvas.text(pos2(at.left, at.upper), Align2::LEFT_CENTER, title, title_font, label());
    if !song.artist.is_empty() && at.reveal > 0.01 {
        let room = remaining_left - TEXT_GAP - at.left;
        let artist = controls::truncate(canvas, &song.artist, font::regular(11.0), room);
        let tint = secondary_label().gamma_multiply(at.reveal);
        canvas.text(pos2(at.left, at.lower), Align2::LEFT_CENTER, artist, font::regular(11.0), tint);
    }
    slide_words(canvas, mark, part, at.left..=at.time_right);
}

fn paint_notch_words(canvas: &mut Canvas, song: &NowPlaying, at: &NotchWords, part: Part, now: Instant) {
    if part.alpha <= 0.01 {
        return;
    }
    let mark = canvas.mark();
    let right = paint_remaining(canvas, song, pos2(at.right, at.y), now, at.detail);
    let title = canvas.mark();
    paint_title_line(canvas, song, pos2(at.left, at.y), right - TEXT_GAP - at.left);
    for shape in canvas.since(title) {
        fade(shape, at.reveal);
    }
    slide_words(canvas, mark, part, at.left..=at.right);
}

fn fade_since(canvas: &mut Canvas, mark: Mark, alpha: f32) {
    if alpha >= 1.0 {
        return;
    }
    for shape in canvas.since(mark) {
        fade(shape, alpha);
    }
}

fn slide_words(canvas: &mut Canvas, mark: Mark, part: Part, column: std::ops::RangeInclusive<f32>) {
    let shift = vec2(part.shift * WORDS_SHIFT, 0.0);
    for shape in canvas.since(mark) {
        shape.translate(shift);
        fade(shape, part.alpha);
    }
    if part.shift != 0.0 {
        canvas.clip(mark, Rect::from_x_y_ranges(column, f32::MIN..=f32::MAX));
    }
}

fn paint_cover(canvas: &mut Canvas, music: &Music, frame: Rect, radius: f32, tint: Color32, now: Instant) {
    let arriving = music.cover.arriving(now).alpha;
    if let Some((art, _)) = music.cover.leaving(now) {
        let covered = music.cover.shown().is_some_and(|art| art.texture.is_some());
        let under = if covered { 1.0 } else { 1.0 - arriving };
        paint_art(canvas, art, tint, frame, radius, ArtLook { alpha: under, ..ArtLook::resting(music.now.playing) });
    }
    let look = look(music.pulse, music.now.playing, now);
    let settling =
        ArtLook { alpha: look.alpha * arriving, scale: look.scale * (1.0 - COVER_SETTLE * (1.0 - arriving)), ..look };
    paint_art(canvas, music.cover.shown(), tint, frame, radius, settling);
}

fn paint_art(canvas: &mut Canvas, art: Option<&Art>, tint: Color32, frame: Rect, radius: f32, look: ArtLook) {
    if look.alpha <= 0.01 {
        return;
    }
    let center = frame.center() + vec2(look.slide * frame.width(), 0.0);
    let rect = Rect::from_center_size(center, frame.size() * look.scale);
    if let Some((reach, alpha)) = look.ring.filter(|(_, alpha)| *alpha > 0.01) {
        let spread = reach * frame.width();
        canvas.rect_stroke(rect.expand(spread), radius + spread, Stroke::new(1.5, tint.gamma_multiply(alpha)));
    }
    let shade = (look.brightness * 255.0).round() as u8;
    let lit = Color32::from_gray(shade).gamma_multiply(look.alpha);
    match art.and_then(|art| art.texture.as_ref()) {
        Some(texture) if look.warp > WARP_VISIBLE => {
            paint_warped(canvas, texture.id(), rect, radius, lit, &Warp { strength: look.warp, phase: look.phase });
        }
        Some(texture) => {
            let corner = CornerRadius::same(radius.round() as u8);
            canvas.add(RectShape::filled(rect, corner, lit).with_texture(texture.id(), FULL_UV));
            canvas.rect_stroke(rect, radius, Stroke::new(1.0, white(0.1 * look.alpha)));
        }
        None => {
            canvas.rect(rect, radius, color::mix(white(0.14), tint, 0.25).gamma_multiply(look.alpha));
            glyph::paint(canvas, Glyph::Note, rect.center(), rect.height() * 0.55, white(0.8 * look.alpha));
        }
    }
    if let Some((glyph, alpha, share)) = look.glyph.filter(|(_, alpha, _)| *alpha > 0.01) {
        canvas.rect(rect, radius, Color32::from_black_alpha((SCRIM * alpha * 255.0) as u8));
        glyph::paint(canvas, glyph, rect.center(), rect.height() * share, white(alpha));
    }
}

fn paint_title_line(canvas: &mut Canvas, now: &NowPlaying, left_center: Pos2, room: f32) {
    let (title_font, artist_font) = (font::semibold(12.0), font::regular(12.0));
    let title_w = canvas.text_width(now.title.as_str(), title_font.clone());
    let artist_w = canvas.text_width(now.artist.as_str(), artist_font.clone());
    if !now.artist.is_empty() && title_w + ARTIST_GAP + artist_w <= room {
        canvas.text(left_center, Align2::LEFT_CENTER, now.title.as_str(), title_font, label());
        let at = pos2(left_center.x + title_w + ARTIST_GAP, left_center.y);
        canvas.text(at, Align2::LEFT_CENTER, now.artist.as_str(), artist_font, secondary_label());
    } else {
        let title = controls::truncate(canvas, &now.title, title_font.clone(), room.max(0.0));
        canvas.text(left_center, Align2::LEFT_CENTER, title, title_font, label());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remaining_counts_whole_seconds() {
        assert_eq!(remaining_label(151.2), "-2:32");
        assert_eq!(remaining_label(0.0), "-0:00");
        assert_eq!(remaining_label(-3.0), "-0:00");
    }
}
