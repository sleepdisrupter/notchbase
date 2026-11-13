//! What the tiles show on their own while something goes on: a browser's downloads, as a thin bar where its running
//! dot sits (the same bar a folder of downloads shows), and a playing song, as the player's icon swelling on the beat.

use egui::{Pos2, Rect, pos2, vec2};
use notchbase_paint::color::white;
use notchbase_paint::spring::{Spring, SpringConfig, UNIT_PRECISION, lerp};

use super::Dock;
use crate::downloads::{Family, Fetching, family_of_app};
use crate::pulse::SWELL;
use crate::tiles::{self, Kind, Tile};

const FILL: SpringConfig = SpringConfig::new(0.5, 1.0);
/// The quick fill to the end when a download finishes.
const FILL_UP: SpringConfig = SpringConfig::new(0.22, 1.0);
const SHOW: SpringConfig = SpringConfig::new(0.3, 1.0);
/// How long the full bar stays after a download finishes, before it fades and the dot comes back.
const FULL_FOR: f32 = 0.45;
/// The bar never looks empty once something is coming.
const LEAST: f32 = 0.04;
const WIDTH: f32 = 16.0;
const HEIGHT: f32 = 3.0;

/// One browser's downloads, on its tile.
pub(super) struct Bar {
    key: String,
    family: Family,
    fill: Spring,
    /// How far along isn't known: a light glides along the bar instead.
    glide: bool,
    shown: Spring,
    /// Seconds since every download in it finished.
    finished: Option<f32>,
}

impl Dock {
    /// What the browsers are fetching now. Each family's bar goes on the running browser of that family that was in
    /// front most recently (`recent` lists pids, most recent first).
    pub fn set_downloads(&mut self, now: &[Fetching], recent: &[i32]) {
        for f in now {
            let key = self.browser_for(f.family, recent);
            let bar = match self.bars.iter_mut().position(|b| b.family == f.family) {
                Some(i) => &mut self.bars[i],
                None => {
                    let Some(key) = key.clone() else { continue };
                    let mut shown = Spring::new(0.0, SHOW, UNIT_PRECISION);
                    shown.animate_to(1.0, SHOW);
                    let fill = Spring::new(f.fraction.unwrap_or(0.0).max(LEAST), FILL, UNIT_PRECISION);
                    self.bars.push(Bar { key, family: f.family, fill, glide: true, shown, finished: None });
                    self.bars.last_mut().expect("just pushed")
                }
            };
            if let Some(key) = key {
                bar.key = key;
            }
            if bar.finished.take().is_some() {
                bar.shown.animate_to(1.0, SHOW);
            }
            bar.glide = f.fraction.is_none();
            if let Some(x) = f.fraction {
                bar.fill.animate_to(x.max(LEAST), FILL);
            }
        }
        let mut done = Vec::new();
        for bar in &mut self.bars {
            if bar.finished.is_none() && !now.iter().any(|f| f.family == bar.family) {
                bar.finished = Some(0.0);
                bar.glide = false;
                bar.fill.animate_to(1.0, FILL_UP);
                done.push(bar.key.clone());
            }
        }
        // A soft swell of the browser's icon as it finishes, like a launch's but smaller.
        for key in done {
            if let Some(t) = self.tiles.iter_mut().find(|t| t.key == key) {
                t.gulp.velocity = tiles::GULP_KICK * 0.35;
                t.gulp.animate_to(0.0, tiles::SWELL);
            }
        }
    }

    fn browser_for(&self, family: Family, recent: &[i32]) -> Option<String> {
        let rank = |pid: i32| recent.iter().position(|p| *p == pid).unwrap_or(usize::MAX);
        self.tiles
            .iter()
            .filter(|t| t.kind == Kind::App && !t.leaving && family_of_app(&t.path) == Some(family))
            .filter_map(|t| Some((rank(t.pid?), t)))
            .min_by_key(|(r, _)| *r)
            .map(|(_, t)| t.key.clone())
    }

    pub(super) fn step_bars(&mut self, dt: f32) {
        for bar in &mut self.bars {
            bar.fill.step(dt);
            bar.shown.step(dt);
            if let Some(t) = &mut bar.finished {
                *t += dt;
                if *t > FULL_FOR && bar.shown.target > 0.0 {
                    bar.shown.animate_to(0.0, SHOW);
                }
            }
        }
        self.bars.retain(|b| b.finished.is_none_or(|t| t <= FULL_FOR || b.shown.value > 0.01));
        // A browser that quit takes its bar with it.
        let keys: Vec<String> = self.tiles.iter().filter(|t| t.pid.is_some()).map(|t| t.key.clone()).collect();
        self.bars.retain(|b| keys.contains(&b.key));
    }

    /// How much of the tile's bar shows, 0 to 1: its running dot gives way to it.
    pub(super) fn bar_shown(&self, key: &str) -> f32 {
        self.bars.iter().find(|b| b.key == key).map_or(0.0, |b| b.shown.value.clamp(0.0, 1.0))
    }

    pub(super) fn paint_tile_bar(&self, painter: &egui::Painter, key: &str, at: Pos2) {
        if let Some(b) = self.bars.iter().find(|b| b.key == key) {
            let fill = (!b.glide).then_some(b.fill.value.clamp(0.0, 1.0));
            paint_bar(painter, at, fill, b.shown.value.clamp(0.0, 1.0), self.time);
        }
    }

    /// The player playing and its beat now, 0 to 1, from the app.
    pub fn set_beat(&mut self, beat: Option<(i32, f32)>) {
        self.beat = beat;
    }

    /// How much bigger the icon is drawn on the beat.
    pub(super) fn beat_of(&self, t: &Tile) -> f32 {
        match self.beat {
            Some((pid, level)) if t.pid == Some(pid) => 1.0 + SWELL * level.clamp(0.0, 1.0),
            _ => 1.0,
        }
    }

    /// Something moves on its own (a download bar, a beat) and wants frames, though nothing else does.
    pub fn ambient(&self) -> bool {
        !self.bars.is_empty() || self.beat.is_some()
    }
}

/// The thin bar under a tile: a track with the part done filled in, or a light gliding along it when how far along
/// isn't known.
pub(super) fn paint_bar(painter: &egui::Painter, at: Pos2, fill: Option<f32>, alpha: f32, time: f32) {
    if alpha <= 0.0 {
        return;
    }
    let track = Rect::from_center_size(at, vec2(WIDTH, HEIGHT));
    painter.rect_filled(track, HEIGHT * 0.5, white(0.16 * alpha));
    match fill {
        Some(f) => {
            let done = Rect::from_min_size(track.min, vec2((WIDTH * f).max(HEIGHT), HEIGHT));
            painter.rect_filled(done, HEIGHT * 0.5, white(0.8 * alpha));
        }
        None => {
            let phase = 0.5 - 0.5 * (time * std::f32::consts::TAU / 1.4).cos();
            let x = lerp(track.left() + 3.0, track.right() - 3.0, phase);
            painter.rect_filled(
                Rect::from_center_size(pos2(x, at.y), vec2(6.0, HEIGHT)),
                HEIGHT * 0.5,
                white(0.75 * alpha),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use egui::vec2;

    use super::*;
    use crate::config::Config;
    use crate::os::Running;

    const DT: f32 = 1.0 / 60.0;

    fn running(apps: &[(i32, &str)]) -> Vec<Running> {
        apps.iter()
            .map(|&(pid, name)| Running {
                pid,
                path: Some(PathBuf::from(format!("/Applications/{name}.app"))),
                name: name.into(),
                active: false,
                hidden: false,
            })
            .collect()
    }

    fn dock(apps: &[(i32, &str)]) -> Dock {
        let dir = std::env::temp_dir().join("notchbase-dock-ambient-tests");
        let mut d = Dock::new(Config::default(), dir, vec2(1000.0, super::super::WINDOW_HEIGHT));
        d.refresh(&running(apps));
        d
    }

    fn key_of(d: &Dock, pid: i32) -> String {
        d.tiles.iter().find(|t| t.pid == Some(pid)).map(|t| t.key.clone()).expect("tile")
    }

    fn run(d: &mut Dock, seconds: f32) {
        for _ in 0..(seconds / DT) as usize {
            d.step_bars(DT);
        }
    }

    #[test]
    fn a_download_shows_on_its_browser_fills_and_goes_once_done() {
        let mut d = dock(&[(7, "Safari"), (8, "Notes")]);
        let safari = key_of(&d, 7);
        d.set_downloads(&[Fetching { family: Family::Safari, fraction: Some(0.6) }], &[]);
        run(&mut d, 1.5);
        assert!(d.bar_shown(&safari) > 0.99);
        assert_eq!(d.bar_shown(&key_of(&d, 8)), 0.0);
        let bar = &d.bars[0];
        assert!((bar.fill.value - 0.6).abs() < 0.01 && !bar.glide);

        d.set_downloads(&[], &[]);
        run(&mut d, 0.3);
        assert!(d.bars[0].fill.value > 0.95, "it fills to the end first");
        assert!(d.bar_shown(&safari) > 0.99, "and holds there a moment");
        run(&mut d, 2.0);
        assert!(d.bars.is_empty());
        assert!(!d.ambient());
    }

    #[test]
    fn a_download_of_unknown_size_glides() {
        let mut d = dock(&[(7, "Firefox")]);
        d.set_downloads(&[Fetching { family: Family::Firefox, fraction: None }], &[]);
        assert!(d.bars[0].glide);
        d.set_downloads(&[Fetching { family: Family::Firefox, fraction: Some(0.2) }], &[]);
        assert!(!d.bars[0].glide);
    }

    #[test]
    fn the_bar_goes_on_the_browser_in_front_most_recently() {
        let mut d = dock(&[(7, "Google Chrome"), (9, "Arc")]);
        d.set_downloads(&[Fetching { family: Family::Chromium, fraction: Some(0.1) }], &[3, 9, 7]);
        assert_eq!(d.bars.len(), 1);
        assert_eq!(d.bars[0].key, key_of(&d, 9));
    }

    #[test]
    fn nothing_shows_when_no_browser_of_that_kind_is_running() {
        let mut d = dock(&[(8, "Notes")]);
        d.set_downloads(&[Fetching { family: Family::Safari, fraction: Some(0.5) }], &[]);
        assert!(d.bars.is_empty());
    }

    #[test]
    fn only_the_playing_apps_icon_swells_and_only_a_little() {
        let mut d = dock(&[(7, "Spotify"), (8, "Notes")]);
        d.set_beat(Some((7, 1.0)));
        let spotify = d.tiles.iter().find(|t| t.pid == Some(7)).unwrap();
        let notes = d.tiles.iter().find(|t| t.pid == Some(8)).unwrap();
        assert!((d.beat_of(spotify) - 1.06).abs() < 1e-6);
        assert_eq!(d.beat_of(notes), 1.0);
        d.set_beat(Some((7, 7.0)));
        assert!(d.beat_of(d.tiles.iter().find(|t| t.pid == Some(7)).unwrap()) <= 1.06 + 1e-6);
        d.set_beat(None);
        assert!(!d.ambient());
    }
}
