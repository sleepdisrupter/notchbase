//! The sample song the previews pin.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::media::{Bands, NowPlaying, Timeline};

use super::super::Notch;
use super::super::preview::{Preview, sample_artwork};
use super::Music;

const PINNED: Duration = Duration::from_secs(60 * 60 * 24 * 365);
const SAMPLE_TITLE: &str = "Neon Tides";
const SAMPLE_ARTIST: &str = "Harbor Lights";
const SAMPLE_BANDS: Bands = [0.95, 0.7, 0.55, 0.8, 0.45, 0.6];
const SAMPLE_POSITION: f64 = 62.0;
const SAMPLE_LENGTH: f64 = 213.0;
const SAMPLE_VOLUME: f32 = 0.62;

impl Notch {
    pub fn previewed_music(&self) -> Option<Option<NowPlaying>> {
        if self.preview == Some(Preview::MediaQuiet) {
            return Some(None);
        }
        self.preview.and(self.music.as_ref()).map(|music| Some(music.now.clone()))
    }

    pub(in crate::notch) fn pin_sample_music_with_volume(&mut self) {
        self.pin_sample_music(false);
        self.pin_sample_strip(SAMPLE_VOLUME, Instant::now() + PINNED);
    }

    pub(in crate::notch) fn pin_sample_music(&mut self, docked: bool) {
        let now = NowPlaying {
            title: SAMPLE_TITLE.to_owned(),
            artist: SAMPLE_ARTIST.to_owned(),
            artwork: Some(Arc::new(sample_artwork())),
            playing: true,
            timeline: Timeline::new(SAMPLE_POSITION, SAMPLE_LENGTH, Instant::now()),
        };
        self.music = Some(Music::new(now));
        self.music_until = Some(Instant::now() + PINNED);
        self.liquid.set(SAMPLE_BANDS);
        if docked {
            self.hug.snap_to(1.0);
        }
    }
}
