//! Hugging the top edge: the docked shape for music, and tucking the song beside the camera.

use egui::{Pos2, Rect, Vec2, pos2, vec2};

use crate::notch::geometry::{CANVAS, Silhouette};
use crate::notch::{Mode, Notch};
use crate::platform::Style;
use crate::ui::spring::{SpringConfig, lerp};

use super::attract::ATTRACT;

pub(in crate::notch) const DOCK_H: f32 = 24.0;
const DOCK_W: f32 = 176.0;
pub(in crate::notch) const DOCK_EAR: f32 = 5.0;
pub(super) const TUCKED_WING: f32 = 36.0;
pub(in crate::notch) const GLIDE: SpringConfig = SpringConfig::new(0.46, 0.8);
const SHOWN: f32 = 0.01;
const TUCKED_IN: f32 = 0.9;

impl Notch {
    pub(super) fn hugs_top(&self) -> bool {
        let music = self.shows_music() && self.strip.is_none();
        let live = self.style == Style::Island && self.shows_live();
        self.mode == Mode::Idle && !self.pinned && (music || live)
    }

    pub(super) fn hug_toward(&mut self, cursor: Option<Pos2>) -> bool {
        let hug = if self.hugs_top() { 1.0 - cursor.map_or(0.0, |c| self.reach(c)) } else { 0.0 };
        let moved = (self.hug.target - hug).abs() > 1e-3;
        self.hug.animate_to(hug, ATTRACT);
        moved
    }

    pub(super) fn release_hug(&mut self) {
        if !self.hugs_top() {
            self.hug.animate_to(0.0, ATTRACT);
        }
    }

    pub(in crate::notch) fn fold_from_the_tuck(&mut self) {
        let hug = self.hug.value.clamp(0.0, 1.0);
        if self.style != Style::Notch || self.mode != Mode::Idle || hug < TUCKED_IN {
            return;
        }
        let tucked = vec2(self.dock.value, self.docked_size().y);
        self.morph.w.value = lerp(self.morph.w.value, tucked.x, hug);
        self.morph.h.value = lerp(self.morph.h.value, tucked.y, hug);
    }

    pub(super) fn docked_size(&self) -> Vec2 {
        match self.style {
            Style::Notch => vec2(self.notch.x + 2.0 * TUCKED_WING, self.notch.y),
            Style::Island if self.shows_live() => vec2(self.live_geometry().w, DOCK_H),
            Style::Island => vec2(DOCK_W, DOCK_H),
        }
    }

    pub(super) fn follow_dock(&mut self) {
        let target = self.docked_size().x;
        if self.hug.value <= SHOWN {
            self.dock.snap_to(target);
        } else if (self.dock.target - target).abs() > 1e-3 {
            self.dock.animate_to(target, GLIDE);
        }
    }

    pub(in crate::notch) fn hugged(&self, s: Silhouette) -> Silhouette {
        let hug = self.hug.value.clamp(0.0, 1.0);
        if hug <= 0.0 {
            return s;
        }
        let dock = vec2(self.dock.value, self.docked_size().y);
        match self.style {
            Style::Notch => tucked(s, hug, dock),
            Style::Island => docked(s, hug, dock),
        }
    }

    pub(super) fn docked_rect(&self) -> Rect {
        let size = self.docked_size();
        Rect::from_min_size(pos2(CANVAS.x * 0.5 - size.x * 0.5, 0.0), size)
    }
}

fn tucked(s: Silhouette, hug: f32, size: Vec2) -> Silhouette {
    let h = lerp(s.h, size.y, hug);
    Silhouette { w: lerp(s.w, size.x, hug), h, bottom_r: s.bottom_r.min(h * 0.5), ..s }
}

fn docked(s: Silhouette, hug: f32, dock: Vec2) -> Silhouette {
    let lift = (hug / 0.5).min(1.0);
    let ears = ((hug - 0.5) / 0.5).clamp(0.0, 1.0);
    let h = lerp(s.h, dock.y, hug);
    Silhouette {
        top: lerp(s.top, 0.0, lift),
        w: lerp(s.w, dock.x, hug),
        h,
        top_r: lerp(s.top_r, 0.0, lift),
        bottom_r: s.bottom_r.min(h * 0.5),
        ear: DOCK_EAR * ears,
        bleed_top: s.bleed_top || lift >= 1.0,
        ..s
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::notch::tests::fixtures::{AWAY, REST, island, settle, song};

    #[test]
    fn music_hugs_top() {
        let mut n = island();
        let t0 = Instant::now();
        assert!(n.set_now_playing(Some(&song(true, 10.0))));
        n.tick(t0, Some(AWAY));
        settle(&mut n);
        assert_eq!(n.hug.value, 1.0);
        let docked = n.silhouette();
        assert_eq!((docked.top, docked.h, docked.w), (0.0, DOCK_H, DOCK_W));
        assert!(docked.bleed_top && docked.ear > 0.0);

        let near = pos2(CANVAS.x * 0.5, DOCK_H + 10.0);
        n.tick(t0, Some(pos2(near.x, 0.0)));
        settle(&mut n);
        n.tick(t0, Some(near));
        settle(&mut n);
        n.tick(t0, Some(near));
        n.tick(t0 + REST, Some(near));
        settle(&mut n);
        assert_eq!((n.hug.value, n.mode), (0.0, Mode::Peek));
        assert_eq!(n.silhouette().rect(), n.body_rect(n.peek_of(n.music_geometry())));
    }

    #[test]
    fn song_tucks_beside_camera() {
        let mut n = Notch::with_preview(Style::Notch, Some(vec2(185.0, 32.0)), None);
        n.set_now_playing(Some(&song(true, 10.0)));
        n.tick(Instant::now(), Some(AWAY));
        settle(&mut n);
        assert_eq!(n.hug.value, 1.0);
        let tucked = n.silhouette();
        assert_eq!((tucked.h, tucked.w, tucked.top), (32.0, 185.0 + 2.0 * TUCKED_WING, 0.0));
        let (t0, below) = (Instant::now(), pos2(CANVAS.x * 0.5, 45.0));
        n.tick(t0, Some(below));
        settle(&mut n);
        n.tick(t0, Some(below));
        n.tick(t0 + REST, Some(below));
        settle(&mut n);
        assert_eq!((n.hug.value, n.mode), (0.0, Mode::Idle), "below the camera the song comes down but nothing opens");
        let on = pos2(CANVAS.x * 0.5, 20.0);
        n.tick(t0 + REST, Some(on));
        n.tick(t0 + REST * 2, Some(on));
        settle(&mut n);
        assert_eq!((n.hug.value, n.mode), (0.0, Mode::Peek));
        assert_eq!(n.silhouette().h, n.peek_of(n.music_geometry()).h);
    }

    #[test]
    fn half_docked_is_smooth() {
        let n = island();
        let floating = Silhouette {
            center_x: 340.0,
            top: 10.0,
            w: 250.0,
            h: 48.0,
            top_r: 24.0,
            bottom_r: 24.0,
            ear: 0.0,
            bleed_top: false,
            wave: None,
        };
        let dock = n.docked_size();
        let mut prev = floating;
        for i in 1..=100 {
            let s = docked(floating, i as f32 / 100.0, dock);
            assert!((s.w - prev.w).abs() <= 1.0 && (s.h - prev.h).abs() <= 1.0 && (s.top - prev.top).abs() <= 1.0);
            assert!(s.ear == 0.0 || s.top_r == 0.0, "ears only once the top corners are gone");
            prev = s;
        }
        assert_eq!((prev.w, prev.h, prev.top), (dock.x, dock.y, 0.0));
    }
}
