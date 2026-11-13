//! The Media page: the player on top and the sound mixer folded under it.

use egui::{Rect, pos2};

use super::PageCtx;
use super::mixer::MixerPage;
use super::player::PlayerPage;
use crate::audio::Mixer;
use crate::bridge::Action;
use crate::media::NowPlaying;

const GAP: f32 = 10.0;
const MIXER_SCOPE: &str = "media.mixer";

pub struct MediaPage {
    player: PlayerPage,
    mixer: MixerPage,
}

impl Default for MediaPage {
    fn default() -> Self {
        Self { player: PlayerPage::default(), mixer: MixerPage::folded(MIXER_SCOPE) }
    }
}

impl MediaPage {
    pub fn height(&self, now_playing: Option<&NowPlaying>, mixer: &Mixer, width: f32) -> f32 {
        mixer_top(self.player.height(now_playing)) + self.mixer.height(mixer, width)
    }

    pub fn unfold_mixer(&mut self) {
        self.mixer.unfold();
    }

    pub fn is_animating(&self) -> bool {
        self.player.is_animating() || self.mixer.is_animating()
    }

    pub fn ui(&mut self, ctx: &mut PageCtx<'_>, now_playing: Option<&NowPlaying>, mixer: &Mixer) -> Vec<Action> {
        let page = ctx.rect;
        let top = mixer_top(self.player.height(now_playing));
        ctx.rect = Rect::from_min_max(page.min, pos2(page.right(), page.top() + self.player.height(now_playing)));
        let mut actions = self.player.ui(ctx, now_playing);
        ctx.rect = Rect::from_min_max(pos2(page.left(), page.top() + top), page.max);
        actions.extend(self.mixer.ui(ctx, mixer));
        ctx.rect = page;
        actions
    }
}

fn mixer_top(player: f32) -> f32 {
    player + GAP
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folded_mixer_is_one_row() {
        let mut page = MediaPage::default();
        let mixer = Mixer::default();
        let player = PlayerPage::default().height(None);
        let folded = MixerPage::folded(MIXER_SCOPE).height(&mixer, 400.0);
        assert_eq!(page.height(None, &mixer, 400.0), player + GAP + folded);
        assert!(!page.is_animating());
        page.unfold_mixer();
        let everything = MixerPage::default().height(&mixer, 400.0);
        assert!(page.height(None, &mixer, 400.0) > player + GAP + everything);
        assert!(!page.is_animating());
    }
}
