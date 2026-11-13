//! Decides what the UI shows from noisy player reports. A song shows once it's heard playing, lingers 3 seconds after
//! a pause, and keeps its timeline when the player briefly drops it.

use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::ColorImage;

use super::artwork;
use super::track::{NowPlaying, Timeline, Track};

const LINGER: Duration = Duration::from_secs(3);
/// Artwork is fetched at most this many times for each song.
const ARTWORK_TRIES: u8 = 4;

#[derive(Default)]
pub(super) struct Follower {
    last: Option<Track>,
    artwork: Option<Arc<ColorImage>>,
    artwork_tries: u8,
    /// Set once the song is heard playing, so a paused song left from earlier never pops up.
    heard: bool,
    paused_at: Option<Instant>,
    timeline: Option<Timeline>,
}

impl Follower {
    pub(super) fn update(&mut self, track: Option<Track>, now: Instant) -> Option<NowPlaying> {
        let Some(track) = track else {
            *self = Self::default();
            return None;
        };
        if !self.last.as_ref().is_some_and(|last| last.is_same_song(&track)) {
            *self = Self::default();
        }
        self.follow_play_state(track.playing, now);
        self.follow_timeline(track.timeline, track.playing, now);
        self.last = Some(track);
        self.shown(now)
    }

    pub(super) fn wants_artwork(&self) -> bool {
        self.last.is_some() && self.artwork.is_none() && self.artwork_tries < ARTWORK_TRIES
    }

    pub(super) fn attach_artwork(&mut self, bytes: Option<Vec<u8>>, now: Instant) -> Option<NowPlaying> {
        self.artwork_tries += 1;
        self.artwork = bytes.and_then(|bytes| artwork::decode(&bytes)).map(Arc::new);
        self.shown(now)
    }

    fn shown(&self, now: Instant) -> Option<NowPlaying> {
        let track = self.last.as_ref()?;
        let lingering = self.paused_at.is_none_or(|at| now.saturating_duration_since(at) < LINGER);
        (self.heard && lingering).then(|| NowPlaying {
            title: track.title.clone(),
            artist: track.artist.clone(),
            artwork: self.artwork.clone(),
            playing: track.playing,
            timeline: self.timeline,
        })
    }

    fn follow_play_state(&mut self, playing: bool, now: Instant) {
        if playing {
            self.heard = true;
            self.paused_at = None;
        } else {
            self.paused_at.get_or_insert(now);
        }
    }

    fn follow_timeline(&mut self, reported: Option<Timeline>, playing: bool, now: Instant) {
        self.timeline = match (reported, self.timeline) {
            (Some(timeline), _) => Some(timeline),
            (None, Some(known)) => Timeline::new(known.position_at(now, playing), known.duration, now),
            (None, None) => None,
        };
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use image::{ImageFormat, Rgba, RgbaImage};

    use super::*;

    fn track(title: &str, playing: bool) -> Option<Track> {
        Some(Track { title: title.into(), artist: "Artist".into(), playing, artwork_key: title.into(), timeline: None })
    }

    fn png() -> Vec<u8> {
        let mut bytes = Vec::new();
        RgbaImage::from_pixel(8, 8, Rgba([200, 30, 90, 255]))
            .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
            .unwrap();
        bytes
    }

    #[test]
    fn song_shows_then_artwork_follows() {
        let mut follower = Follower::default();
        let t0 = Instant::now();
        let first = follower.update(track("A", true), t0).expect("shown");
        assert!(first.playing && first.artwork.is_none(), "the title doesn't wait for the artwork");
        assert!(follower.wants_artwork());
        let with_art = follower.attach_artwork(Some(png()), t0).expect("shown");
        assert!(with_art.artwork.is_some());
        assert!(!follower.wants_artwork());
        let again = follower.update(track("A", true), t0).expect("shown");
        assert!(again.same_as(&with_art));
        let next = follower.update(track("B", true), t0).expect("shown");
        assert_eq!(next.title, "B");
        assert!(next.artwork.is_none() && follower.wants_artwork());
    }

    #[test]
    fn missing_artwork_is_retried() {
        let mut follower = Follower::default();
        let t0 = Instant::now();
        let mut fetches = 0;
        for _ in 0..10 {
            follower.update(track("A", true), t0);
            if follower.wants_artwork() {
                fetches += 1;
                follower.attach_artwork(None, t0);
            }
        }
        assert_eq!(fetches, usize::from(ARTWORK_TRIES));
    }

    #[test]
    fn pause_lingers_then_hides() {
        let mut follower = Follower::default();
        let t0 = Instant::now();
        follower.update(track("A", true), t0);
        let paused = follower.update(track("A", false), t0 + Duration::from_secs(1)).expect("lingers");
        assert!(!paused.playing);
        assert!(follower.update(track("A", false), t0 + Duration::from_secs(3)).is_some());
        assert!(follower.update(track("A", false), t0 + Duration::from_millis(4_100)).is_none());
        assert!(follower.update(track("A", true), t0 + Duration::from_secs(5)).is_some());
    }

    #[test]
    fn timeline_survives_a_brief_drop() {
        let mut follower = Follower::default();
        let t0 = Instant::now();
        let timed = |position| track("A", true).map(|t| Track { timeline: Timeline::new(position, 200.0, t0), ..t });
        follower.update(timed(30.0), t0);
        let later = t0 + Duration::from_secs(2);
        let kept = follower.update(track("A", true), later).and_then(|now| now.timeline).expect("kept");
        assert!((kept.position_at(later, true) - 32.0).abs() < 0.01);
        let next = follower.update(track("B", true), later).expect("shown");
        assert!(next.timeline.is_none(), "a new song starts without the old timeline");
    }

    #[test]
    fn paused_song_stays_hidden() {
        let mut follower = Follower::default();
        let t0 = Instant::now();
        assert!(follower.update(track("A", false), t0).is_none());
        assert!(follower.update(None, t0).is_none());
        assert!(follower.update(track("A", true), t0).is_some());
    }
}
