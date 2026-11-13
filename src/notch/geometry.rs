//! Sizes and shapes: the canvas, the notch and island silhouettes, and the springs that morph between them.

use egui::{CornerRadius, Rect, Vec2, pos2, vec2};

use crate::platform::Style;
use crate::ui::metrics::PAD;
use crate::ui::motion;
use crate::ui::shape::{Blob, CORNER_REACH, Wave};
use crate::ui::spring::{POINT_PRECISION, Spring, SpringConfig, lerp};

use super::activity::ActivityKind;
use super::{Mode, Notch};

pub(super) const CANVAS: Vec2 = vec2(680.0, 600.0);
pub(super) const DEFAULT_NOTCH: Vec2 = vec2(185.0, 32.0);
pub(super) const TUCKED: Vec2 = vec2(84.0, 5.0);
const SWOLLEN: Vec2 = vec2(104.0, 14.0);
const ISLAND_TOP: f32 = 10.0;
const PANEL_WIDTH: f32 = 372.0;
const NOTCH_PANEL_WINGS: f32 = 190.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Geometry {
    pub(super) w: f32,
    pub(super) h: f32,
    pub(super) r: f32,
    pub(super) ear: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Silhouette {
    pub(super) center_x: f32,
    pub(super) top: f32,
    pub(super) w: f32,
    pub(super) h: f32,
    pub(super) top_r: f32,
    pub(super) bottom_r: f32,
    pub(super) ear: f32,
    pub(super) bleed_top: bool,
    pub(super) wave: Option<Wave>,
}

impl Silhouette {
    pub(super) fn blob(&self) -> Blob {
        Blob {
            rect: Rect::from_min_size(pos2(self.center_x - self.w * 0.5, self.top), vec2(self.w, self.h)),
            top_radius: self.top_r,
            bottom_radius: self.bottom_r,
            ear: self.ear,
            bleed_top: self.bleed_top,
            wave: self.wave,
        }
    }

    pub(super) fn docking(&self) -> f32 {
        if self.bleed_top { 1.0 } else { 1.0 - (self.top / ISLAND_TOP).clamp(0.0, 1.0) }
    }

    pub(super) fn rect(&self) -> Rect {
        self.blob().rect
    }

    pub(super) fn bounds(&self) -> Rect {
        self.blob().bounds()
    }

    pub(super) fn corner_radius(&self) -> CornerRadius {
        let radius = |r: f32| r.round().clamp(0.0, 255.0) as u8;
        let (top, bottom) = (radius(self.top_r), radius(self.bottom_r));
        CornerRadius { nw: top, ne: top, sw: bottom, se: bottom }
    }
}

fn notch_silhouette(g: Geometry) -> Silhouette {
    Silhouette {
        center_x: CANVAS.x * 0.5,
        top: 0.0,
        w: g.w,
        h: g.h,
        top_r: 0.0,
        bottom_r: g.r,
        ear: g.ear,
        bleed_top: true,
        wave: None,
    }
}

fn island_silhouette(g: Geometry, attract: f32, lean: f32) -> Silhouette {
    let attract = attract.clamp(0.0, 1.0);
    let center_x = CANVAS.x * 0.5 + lean;
    let stretch = lean.abs() * 0.25;
    if attract < 0.5 {
        let t = attract / 0.5;
        Silhouette {
            center_x,
            top: 0.0,
            w: lerp(TUCKED.x, SWOLLEN.x, t) + stretch,
            h: lerp(TUCKED.y, SWOLLEN.y, t),
            top_r: 0.0,
            bottom_r: lerp(2.5, SWOLLEN.y * 0.5, t),
            ear: lerp(2.5, 0.0, t),
            bleed_top: true,
            wave: None,
        }
    } else {
        let u = (attract - 0.5) / 0.5;
        let (w, h) = (lerp(SWOLLEN.x, g.w, u) + stretch, lerp(SWOLLEN.y, g.h, u));
        // The shape is only 14 points tall when it lets go of the edge, so a plain lerp of the radius
        // fills its corners within a few frames and the flat top snaps round. Let the corners take only
        // as much of their room as the island has dropped, so the top rounds off as it comes down. Once
        // it's out this is the same corner the full radius draws, since a corner never runs past its room.
        let room = 0.5 * w.min(h) / CORNER_REACH;
        Silhouette {
            center_x,
            top: lerp(0.0, ISLAND_TOP, u),
            w,
            h,
            top_r: lerp(0.0, g.r, u).min(room * u),
            bottom_r: lerp(SWOLLEN.y * 0.5, g.r, u),
            ear: 0.0,
            bleed_top: u <= 0.0,
            wave: None,
        }
    }
}

pub(super) struct Morph {
    pub(super) w: Spring,
    pub(super) h: Spring,
    pub(super) r: Spring,
    pub(super) ear: Spring,
    pub(super) shift: Spring,
}

impl Default for Morph {
    fn default() -> Self {
        let px = || Spring::new(0.0, motion::HOVER, POINT_PRECISION);
        Self { w: px(), h: px(), r: px(), ear: px(), shift: px() }
    }
}

impl Morph {
    pub(super) fn current(&self) -> Geometry {
        Geometry {
            w: self.w.value.max(0.0),
            h: self.h.value.max(0.0),
            r: self.r.value.max(0.0),
            ear: self.ear.value.max(0.0),
        }
    }

    pub(super) fn snap_to(&mut self, g: Geometry) {
        self.w.snap_to(g.w);
        self.h.snap_to(g.h);
        self.r.snap_to(g.r);
        self.ear.snap_to(g.ear);
    }

    pub(super) fn animate_to(&mut self, g: Geometry, width: SpringConfig, height: SpringConfig) {
        self.w.animate_to(g.w, width);
        self.h.animate_to(g.h, height);
        self.r.animate_to(g.r, height);
        self.ear.animate_to(g.ear, height);
    }

    pub(super) fn springs(&self) -> [&Spring; 5] {
        [&self.w, &self.h, &self.r, &self.ear, &self.shift]
    }

    pub(super) fn springs_mut(&mut self) -> [&mut Spring; 5] {
        [&mut self.w, &mut self.h, &mut self.r, &mut self.ear, &mut self.shift]
    }
}

impl Notch {
    pub(super) fn idle_geometry(&self) -> Geometry {
        match self.style {
            Style::Notch => Geometry { w: self.notch.x, h: self.notch.y, r: 9.0, ear: 4.0 },
            Style::Island => Geometry { w: 126.0, h: 37.0, r: 18.5, ear: 0.0 },
        }
    }

    pub(super) fn panel_width(&self) -> f32 {
        match self.style {
            Style::Notch => (self.notch.x + NOTCH_PANEL_WINGS).max(PANEL_WIDTH),
            Style::Island => PANEL_WIDTH,
        }
    }

    pub(super) fn expanded_geometry(&self) -> Geometry {
        let h = self.header_height() + self.download_room() + self.pager.height(self.pager.page) + PAD;
        let (r, ear) = match self.style {
            Style::Notch => (24.0, 12.0),
            Style::Island => (34.0, 0.0),
        };
        Geometry { w: self.panel_width(), h, r, ear }
    }

    pub(super) fn peek_of(&self, g: Geometry) -> Geometry {
        match self.style {
            Style::Notch => Geometry { w: g.w + 18.0, h: g.h + 5.0, r: g.r + 2.0, ear: g.ear + 2.0 },
            Style::Island => {
                let h = g.h + 2.0;
                Geometry { w: g.w + 8.0, h, r: h * 0.5, ear: 0.0 }
            }
        }
    }

    pub(super) fn rest_geometry(&self) -> Geometry {
        if self.activity.is_live() {
            self.activity_geometry()
        } else if self.live.is_some() {
            self.live_geometry()
        } else if self.music_revealed() {
            self.music_geometry()
        } else {
            self.idle_geometry()
        }
    }

    pub(super) fn target_geometry(&self) -> Geometry {
        match self.mode {
            Mode::Idle | Mode::Activity => self.rest_geometry(),
            Mode::Peek => self.peek_of(self.rest_geometry()),
            Mode::Expanded => self.expanded_geometry(),
            Mode::Drop => self.drop_geometry(),
            Mode::Ask => self.ask_geometry(),
        }
    }

    pub(super) fn target_shift(&self) -> f32 {
        match (self.mode, &self.activity.kind) {
            (Mode::Idle | Mode::Activity | Mode::Peek, Some(ActivityKind::Tool(notice))) if self.activity.is_live() => {
                self.notice_shift(notice)
            }
            _ => 0.0,
        }
    }

    pub(super) fn body_rect(&self, g: Geometry) -> Rect {
        let top = match self.style {
            Style::Notch => 0.0,
            Style::Island => ISLAND_TOP,
        };
        Rect::from_min_size(pos2(CANVAS.x * 0.5 - g.w * 0.5, top), vec2(g.w, g.h))
    }

    pub(super) fn bounds(&self, g: Geometry) -> Rect {
        self.body_rect(g).expand2(vec2(g.ear, 0.0))
    }

    pub(super) fn silhouette(&self) -> Silhouette {
        let g = self.morph.current();
        let silhouette = match self.style {
            Style::Notch => notch_silhouette(g),
            Style::Island => island_silhouette(g, self.attract.value, self.lean.value),
        };
        let silhouette = self.hugged(silhouette);
        Silhouette {
            center_x: silhouette.center_x
                + self.jolt.value
                + self.flee.value
                + self.dodge.value
                + self.morph.shift.value,
            h: silhouette.h + self.launch_swell(self.open_progress(g)),
            wave: self.music_wave(g),
            ..silhouette
        }
    }

    pub(super) fn open_progress(&self, g: Geometry) -> f32 {
        let (idle, open) = (self.idle_geometry(), self.expanded_geometry());
        ((g.h - idle.h) / (open.h - idle.h)).clamp(0.0, 1.0)
    }

    pub(super) fn openness(&self, g: Geometry) -> f32 {
        self.open_progress(g).max(self.panel_fade.value.clamp(0.0, 1.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notch::tests::fixtures::island;
    use crate::ui::shape;

    #[test]
    fn island_shape_is_continuous() {
        let n = island();
        let capsule = n.idle_geometry();
        let tucked = island_silhouette(capsule, 0.0, 0.0);
        assert_eq!(tucked.rect(), Rect::from_min_size(pos2(340.0 - 42.0, 0.0), TUCKED));
        assert!(tucked.bleed_top && tucked.ear > 0.0 && tucked.top_r == 0.0);
        let out = island_silhouette(capsule, 1.0, 0.0);
        assert_eq!(out.rect(), n.body_rect(capsule));
        assert_eq!((out.bottom_r, out.ear, out.bleed_top), (capsule.r, 0.0, false));
        let full = Blob { top_radius: capsule.r, ..out.blob() };
        let (drawn, capsule_outline) = (shape::outline(&out.blob(), 2.0), shape::outline(&full, 2.0));
        assert_eq!(drawn.len(), capsule_outline.len());
        assert!(drawn.iter().zip(&capsule_outline).all(|(a, b)| a.distance(*b) < 0.01), "out, it's the full capsule");

        let fields = |s: &Silhouette| [s.w, s.h, s.top, s.top_r, s.bottom_r, s.ear];
        let mut prev = tucked;
        for i in 1..=100 {
            let s = island_silhouette(capsule, i as f32 / 100.0, 0.0);
            for (a, b) in fields(&prev).into_iter().zip(fields(&s)) {
                assert!((a - b).abs() <= 1.0, "jump of {} at a = {}", (a - b).abs(), i as f32 / 100.0);
            }
            assert!(s.ear == 0.0 || s.top_r == 0.0);
            let rounded = (s.top_r * CORNER_REACH / (0.5 * s.w.min(s.h))).min(1.0);
            let dropped = s.top / ISLAND_TOP;
            assert!(rounded <= dropped + 1e-4, "the top rounds off no faster than it drops: {rounded} at {dropped}");
            prev = s;
        }
    }
}
