//! The little beat the cover makes when a song is played, paused or skipped.

use std::f32::consts::PI;
use std::time::{Duration, Instant};

use crate::media::NowPlaying;
use crate::ui::glyph::Glyph;
use crate::ui::spring::lerp;

use super::super::Notch;

const BEAT_FOR: Duration = Duration::from_millis(650);
const NUDGE_FOR: Duration = Duration::from_millis(380);
const NUDGE_REACH: f32 = 0.25;
const NUDGE_FADE: f32 = 0.45;
const NUDGE_SQUASH: f32 = 0.04;
const PAUSED_DIM: f32 = 0.55;
const RING_ALPHA: f32 = 0.5;
const RING_FROM: f32 = 0.45;
const RING_SPEED: f32 = 1.8;
const SHIMMER_SPEED: f32 = 18.0;
const GLYPH_FROM: f32 = 0.62;
const GLYPH_TO: f32 = 0.4;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::notch) enum Beat {
    Play,
    Pause,
    Nudge(f32),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::notch) struct Pulse {
    beat: Beat,
    at: Instant,
}

impl Pulse {
    pub(super) fn new(beat: Beat, at: Instant) -> Self {
        Self { beat, at }
    }

    fn lasts(&self) -> Duration {
        match self.beat {
            Beat::Nudge(_) => NUDGE_FOR,
            Beat::Play | Beat::Pause => BEAT_FOR,
        }
    }

    fn progress(&self, now: Instant) -> Option<f32> {
        let t = now.saturating_duration_since(self.at).as_secs_f32() / self.lasts().as_secs_f32();
        (t < 1.0).then_some(t.max(0.0))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ArtLook {
    pub(super) scale: f32,
    pub(super) slide: f32,
    pub(super) alpha: f32,
    pub(super) brightness: f32,
    pub(super) warp: f32,
    pub(super) phase: f32,
    pub(super) glyph: Option<(Glyph, f32, f32)>,
    pub(super) ring: Option<(f32, f32)>,
}

impl ArtLook {
    pub(super) fn resting(playing: bool) -> Self {
        let brightness = if playing { 1.0 } else { PAUSED_DIM };
        Self { scale: 1.0, slide: 0.0, alpha: 1.0, brightness, warp: 0.0, phase: 0.0, glyph: None, ring: None }
    }
}

pub(super) fn look(pulse: Option<Pulse>, playing: bool, now: Instant) -> ArtLook {
    let resting = ArtLook::resting(playing);
    let Some((pulse, t)) = pulse.and_then(|pulse| pulse.progress(now).map(|t| (pulse, t))) else { return resting };
    let eased = ease_out(t);
    match pulse.beat {
        Beat::Nudge(direction) => {
            let bump = (PI * t).sin();
            ArtLook {
                slide: -direction * NUDGE_REACH * bump,
                alpha: 1.0 - NUDGE_FADE * bump,
                scale: 1.0 - NUDGE_SQUASH * bump,
                ..resting
            }
        }
        Beat::Play | Beat::Pause => {
            let bump = (PI * t).sin();
            let glyph = if pulse.beat == Beat::Play { Glyph::Play } else { Glyph::Pause };
            let from = if pulse.beat == Beat::Play { PAUSED_DIM } else { 1.0 };
            let closing = (t * RING_SPEED).min(1.0);
            ArtLook {
                brightness: lerp(from, resting.brightness, eased),
                warp: bump,
                phase: t * SHIMMER_SPEED,
                glyph: Some((glyph, bump, lerp(GLYPH_FROM, GLYPH_TO, eased))),
                ring: Some((RING_FROM * (1.0 - ease_out(closing)), RING_ALPHA * (PI * closing).sin())),
                ..resting
            }
        }
    }
}

pub(super) fn skipped(old: &NowPlaying, new: &NowPlaying) -> bool {
    old.title != new.title || old.artist != new.artist
}

pub(super) fn beat_between(old: &NowPlaying, new: &NowPlaying) -> Option<Beat> {
    (old.playing != new.playing).then_some(if new.playing { Beat::Play } else { Beat::Pause })
}

fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}

impl Notch {
    pub(in crate::notch) fn pulse_is_moving(&self) -> bool {
        let now = Instant::now();
        let moves = |music: &super::Music| music.pulse.is_some_and(|p| p.progress(now).is_some()) || music.moves(now);
        self.shows_music() && self.music.as_ref().is_some_and(moves)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pause_implodes_and_dims() {
        let t0 = Instant::now();
        let pulse = Some(Pulse::new(Beat::Pause, t0));
        let middle = look(pulse, false, t0 + BEAT_FOR / 2);
        assert!(
            middle.warp > 0.9 && middle.glyph.is_some_and(|(glyph, alpha, _)| glyph == Glyph::Pause && alpha > 0.9)
        );
        let ring = |at| look(pulse, false, at).ring.map(|(reach, _)| reach).expect("a ring");
        assert!(ring(t0 + BEAT_FOR / 10) > ring(t0 + BEAT_FOR / 3), "the ring closes in on the art");
        let after = look(pulse, false, t0 + BEAT_FOR);
        assert_eq!(after, ArtLook::resting(false));
        assert!(after.brightness < 1.0, "paused art stays a little dim");
    }

    #[test]
    fn beats_follow_changes() {
        let song = |title: &str, playing| NowPlaying {
            title: title.into(),
            artist: "B".into(),
            artwork: None,
            playing,
            timeline: None,
        };
        assert_eq!(beat_between(&song("A", true), &song("A", false)), Some(Beat::Pause));
        assert_eq!(beat_between(&song("A", false), &song("A", true)), Some(Beat::Play));
        assert!(skipped(&song("A", true), &song("B", true)));
        assert!(!skipped(&song("A", true), &song("A", false)));
        assert_eq!(beat_between(&song("A", true), &song("A", true)), None);
    }
}
