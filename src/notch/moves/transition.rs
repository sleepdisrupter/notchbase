//! Moving between modes: the springs for opening, closing, peeking and popping.

use egui::Vec2;

use crate::notch::geometry::DEFAULT_NOTCH;
use crate::notch::{Mode, Notch};
use crate::platform::Style;
use crate::ui::spring::{Spring, SpringConfig};

use super::attract::{ATTRACT, LEAN, resting_attract};

const PANEL_IN: SpringConfig = SpringConfig::new(0.40, 1.0);
const PANEL_OUT: SpringConfig = SpringConfig::new(0.16, 1.0);
const LAYER_IN: SpringConfig = SpringConfig::new(0.30, 1.0);
const LAYER_OUT: SpringConfig = SpringConfig::new(0.16, 1.0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::notch) enum Motion {
    Open,
    Close,
    Dismiss,
    Peek,
    Pop,
    Alert,
    Resize,
    Follow,
}

impl Notch {
    pub(in crate::notch) fn return_to_rest(&mut self) {
        self.return_to_rest_by(Motion::Close);
    }

    pub(in crate::notch) fn return_to_rest_by(&mut self, motion: Motion) {
        if self.pinned {
            self.set_mode(Mode::Expanded, Motion::Open);
        } else {
            self.set_mode(self.rest_mode(), motion);
        }
    }

    fn rest_mode(&self) -> Mode {
        if self.card {
            Mode::Ask
        } else if self.activity.is_live() {
            Mode::Activity
        } else {
            Mode::Idle
        }
    }

    pub(in crate::notch) fn set_mode(&mut self, mode: Mode, motion: Motion) {
        if mode == Mode::Expanded && self.mode != Mode::Expanded && self.preview.is_none() {
            crate::platform::haptic(crate::platform::Feel::Snap);
        }
        self.mode = mode;
        self.dwell_until = None;
        self.leave_until = None;
        self.retarget(motion);
    }

    pub(in crate::notch) fn retarget(&mut self, motion: Motion) {
        let target = self.target_geometry();
        let (width, height) = self.spring_configs(motion);
        self.morph.animate_to(target, width, height);
        self.morph.shift.animate_to(self.target_shift(), width);
        self.downloading.plan_room();

        let open = self.mode == Mode::Expanded;
        let live = self.shows_activity();
        fade(&mut self.panel_fade, open, PANEL_IN, PANEL_OUT);
        fade(&mut self.activity_fade, live, LAYER_IN, LAYER_OUT);
        fade(&mut self.drop_fade, self.mode == Mode::Drop, LAYER_IN, LAYER_OUT);
        fade(&mut self.ask_fade, self.mode == Mode::Ask, LAYER_IN, LAYER_OUT);
        let live = self.shows_live();
        fade(&mut self.live_fade, live, LAYER_IN, LAYER_OUT);
        let music = self.shows_music();
        fade(&mut self.music_fade, music, LAYER_IN, LAYER_OUT);
        self.follow_dance();

        if self.style == Style::Island && self.needs_capsule() && self.preview.is_none() {
            self.attract.animate_to(1.0, ATTRACT);
        }
        if open {
            self.lean.animate_to(0.0, LEAN);
        }
        self.release_hug();

        if self.preview.is_some() {
            self.snap();
        }
    }

    pub(in crate::notch) fn follow_target(&mut self) {
        let (width, height) = (self.morph.w.config, self.morph.h.config);
        self.morph.animate_to(self.target_geometry(), width, height);
        self.morph.shift.animate_to(self.target_shift(), width);
        self.downloading.plan_room();
    }

    pub(in crate::notch) fn remeasured(&mut self, motion: Motion) {
        if self.morph.springs().iter().all(|spring| spring.is_settled()) {
            self.retarget(motion);
        } else {
            self.follow_target();
        }
    }

    pub(in crate::notch) fn follow_dance(&mut self) {
        let dancing = self.dances();
        fade(&mut self.dance, dancing, LAYER_IN, LAYER_OUT);
    }

    pub(in crate::notch) fn spring_configs(&self, motion: Motion) -> (SpringConfig, SpringConfig) {
        let both = |response, damping| (SpringConfig::new(response, damping), SpringConfig::new(response, damping));
        match (self.style, motion) {
            (Style::Notch, Motion::Open) => (SpringConfig::new(0.42, 0.74), SpringConfig::new(0.46, 0.78)),
            (Style::Island, Motion::Open) => (SpringConfig::new(0.40, 0.76), SpringConfig::new(0.46, 0.80)),
            (Style::Notch, Motion::Close) => both(0.34, 0.94),
            (Style::Island, Motion::Close) => both(0.34, 0.95),
            (_, Motion::Dismiss) => both(0.26, 1.0),
            (_, Motion::Peek) => both(0.28, 0.66),
            (_, Motion::Pop) => both(0.38, 0.72),
            (_, Motion::Alert) => both(0.46, 0.52),
            (_, Motion::Resize) => both(0.42, 0.82),
            (_, Motion::Follow) => both(0.1, 1.0),
        }
    }

    pub(in crate::notch) fn springs(&self) -> impl Iterator<Item = &Spring> {
        self.morph
            .springs()
            .into_iter()
            .chain([
                &self.panel_fade,
                &self.activity_fade,
                &self.drop_fade,
                &self.ask_fade,
                &self.live_fade,
                &self.live_hover,
            ])
            .chain([&self.music_fade, &self.dance])
            .chain([&self.activity.pop, &self.activity.swap])
            .chain([
                &self.attract,
                &self.lean,
                &self.hug,
                &self.dock,
                &self.jolt,
                &self.gulp,
                &self.flee,
                &self.fear,
                &self.surge,
                &self.compact,
                &self.dodge,
            ])
            .chain(self.pager.springs())
            .chain(self.tab_pill.springs())
    }

    fn springs_mut(&mut self) -> impl Iterator<Item = &mut Spring> {
        self.morph
            .springs_mut()
            .into_iter()
            .chain([
                &mut self.panel_fade,
                &mut self.activity_fade,
                &mut self.drop_fade,
                &mut self.ask_fade,
                &mut self.live_fade,
                &mut self.live_hover,
                &mut self.music_fade,
                &mut self.dance,
                &mut self.activity.pop,
                &mut self.activity.swap,
            ])
            .chain([
                &mut self.attract,
                &mut self.lean,
                &mut self.hug,
                &mut self.dock,
                &mut self.jolt,
                &mut self.gulp,
                &mut self.flee,
                &mut self.fear,
                &mut self.surge,
                &mut self.compact,
                &mut self.dodge,
            ])
            .chain(self.pager.springs_mut())
            .chain(self.tab_pill.springs_mut())
    }

    pub(in crate::notch) fn step(&mut self, dt: f32) {
        self.follow_dock();
        self.follow_surge();
        self.follow_dodge();
        self.follow_split();
        self.follow_launch();
        for spring in self.springs_mut() {
            spring.step(dt);
        }
        self.pages.step(dt);
        self.liquid.step(dt);
        self.level_meter.step(dt);
        self.step_sides(dt);
        self.step_live_leaving(dt);
        self.step_activity_leaving(dt);
        self.tab_rows.step(dt);
        self.step_spin(dt);
        self.let_the_song_go();
        self.let_the_strip_go();
        self.step_video(dt);
        self.step_downloading(dt);
    }

    pub fn restyle(&mut self, style: Style, notch_size: Option<Vec2>) -> bool {
        let notch = notch_size.unwrap_or(DEFAULT_NOTCH);
        if self.preview.is_some() || (style == self.style && notch == self.notch) {
            return false;
        }
        self.style = style;
        self.notch = notch;
        self.ask_layout = None;
        self.attract.animate_to(resting_attract(style), ATTRACT);
        self.lean.animate_to(0.0, LEAN);
        self.retarget(Motion::Resize);
        self.snap();
        true
    }

    pub(in crate::notch) fn snap(&mut self) {
        for spring in self.springs_mut() {
            spring.snap_to(spring.target);
        }
        self.pages.snap();
        self.downloading.snap();
        self.liquid.snap();
        self.activity.settle();
        self.live_leaving = None;
        self.card_shown = None;
        self.music_leaving = None;
        self.strip_gone = None;
    }
}

fn fade(spring: &mut Spring, shown: bool, fade_in: SpringConfig, fade_out: SpringConfig) {
    if shown {
        spring.animate_to(1.0, fade_in);
    } else {
        spring.animate_to(0.0, fade_out);
    }
}

#[cfg(test)]
mod tests {
    use egui::pos2;

    use super::*;
    use crate::notch::tests::fixtures::{island, settle};

    #[test]
    fn opening_settles() {
        let mut n = island();
        n.set_mode(Mode::Expanded, Motion::Open);
        settle(&mut n);
        assert!(!n.is_animating());
        let open = n.expanded_geometry();
        assert!((n.morph.w.value - open.w).abs() < 1e-3 && (n.morph.h.value - open.h).abs() < 1e-3);
        assert!(n.wants_input(pos2(340.0, 130.0)));
    }

    #[test]
    fn display_change_restyles() {
        let mut n = island();
        n.set_mode(Mode::Expanded, Motion::Open);
        settle(&mut n);
        assert!(!n.restyle(Style::Island, None));
        assert!(n.restyle(Style::Notch, Some(egui::vec2(200.0, 38.0))));
        assert_eq!((n.style, n.notch, n.attract.value), (Style::Notch, egui::vec2(200.0, 38.0), 1.0));
        assert_eq!(n.morph.current(), n.expanded_geometry());
        assert!(n.restyle(Style::Notch, Some(egui::vec2(186.0, 32.0))));
        assert!(n.restyle(Style::Island, None));
        assert_eq!((n.style, n.attract.value, n.mode), (Style::Island, 1.0, Mode::Expanded));
        assert_eq!(n.morph.current(), n.expanded_geometry());
    }
}
