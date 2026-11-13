//! While a song plays, the notch's edge moves with the music (when the setting is on).

use crate::platform::Style;
use crate::ui::spring::{lerp, smoothstep};
use crate::ui::wave::Wave;

use super::super::geometry::Geometry;
use super::super::{Mode, Notch};
use super::{DOCKED_DEPTH, ISLAND_DEPTH, NOTCH_DEPTH};

const GROWN_BY: f32 = 6.0;

impl Notch {
    pub fn dances(&self) -> bool {
        let playing = self.music.as_ref().is_some_and(|music| music.now.playing);
        let busy =
            self.shows_activity() || self.shows_live() || matches!(self.mode, Mode::Expanded | Mode::Ask | Mode::Drop);
        playing && crate::config::current().music.dance && (self.shows_music() || busy) && !self.shows_download()
    }

    pub(in crate::notch) fn music_wave(&self, g: Geometry) -> Option<Wave> {
        let amount = self.music_alpha(g).max(self.dance.value.clamp(0.0, 1.0) * self.grown_past_rest(g));
        if amount <= 0.001 {
            return None;
        }
        let base = match self.style {
            Style::Notch => NOTCH_DEPTH,
            Style::Island => ISLAND_DEPTH,
        };
        let depth = base * lerp(1.0, DOCKED_DEPTH, self.hug.value.clamp(0.0, 1.0));
        Some(self.liquid.wave(depth * amount))
    }

    fn grown_past_rest(&self, g: Geometry) -> f32 {
        match self.style {
            Style::Notch => {
                let idle = self.idle_geometry();
                smoothstep(0.0, 1.0, (g.w - idle.w).max(g.h - idle.h) / GROWN_BY)
            }
            Style::Island if self.mode == Mode::Expanded => 1.0,
            Style::Island => smoothstep(0.8, 1.0, self.attract.value),
        }
    }
}
