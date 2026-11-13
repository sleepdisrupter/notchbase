//! When the song shows on the closed notch: after a change, while the pointer is near, and for a while after.

use std::time::Instant;

use egui::Pos2;

use super::super::{Mode, Notch};
use crate::notch::moves::transition::Motion;

impl Notch {
    pub(in crate::notch) fn music_revealed(&self) -> bool {
        self.music.is_some() && self.music_until.is_some()
    }

    pub(in crate::notch) fn reveal_music_until(&mut self, until: Instant) {
        self.reveal_music(until);
    }

    pub(super) fn reveal_music_now(&mut self) {
        self.reveal_music(Instant::now() + crate::config::current().music.reveal());
    }

    pub(in crate::notch) fn conceal_music(&mut self) {
        self.music_until = None;
        self.arrange();
    }

    pub(in crate::notch) fn summon_music(&mut self, cursor: Option<Pos2>, now: Instant) -> bool {
        let reachable = self.music.is_some() && matches!(self.mode, Mode::Idle | Mode::Peek);
        let Some(cursor) = cursor.filter(|_| reachable) else { return false };
        if self.reach(cursor) <= 0.0 {
            return false;
        }
        let was = self.music_revealed();
        self.reveal_music(now + crate::config::current().music.hover_reveal());
        !was
    }

    pub(in crate::notch) fn expire_music(&mut self, now: Instant) -> bool {
        if self.music_until.is_none_or(|until| now < until) {
            return false;
        }
        self.music_until = None;
        self.tuck_the_song_away();
        true
    }

    pub(super) fn follow_reveal(&mut self, noteworthy: bool) {
        if self.music.is_none() && self.music_until.take().is_some() {
            self.tuck_the_song_away();
        } else if noteworthy {
            self.reveal_music(Instant::now() + crate::config::current().music.reveal());
        }
    }

    fn reveal_music(&mut self, until: Instant) {
        if self.music.is_none() || !crate::config::current().music.in_notch {
            return;
        }
        let was = self.music_revealed();
        self.music_until = Some(self.music_until.map_or(until, |current| current.max(until)));
        self.arrange();
        if !was {
            self.rest_changed(Motion::Pop);
        }
    }

    fn tuck_the_song_away(&mut self) {
        self.arrange();
        self.fold_from_the_tuck();
        self.rest_changed(Motion::Close);
    }

    fn rest_changed(&mut self, motion: Motion) {
        match self.mode {
            Mode::Idle => self.retarget(motion),
            Mode::Peek => self.retarget(Motion::Peek),
            Mode::Activity | Mode::Expanded | Mode::Drop | Mode::Ask => self.remeasured(motion),
        }
    }
}
