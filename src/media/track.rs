//! What's playing: the song, its play state and a timeline that runs on between polls, plus the controls sent back.

use std::sync::Arc;
use std::time::Instant;

use egui::ColorImage;

/// Timelines this close, in seconds, count as the same, so reporting jitter isn't a change.
const SAME_POSITION: f64 = 1.5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Control {
    PlayPause,
    Next,
    Previous,
    Seek(f64),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Timeline {
    pub position: f64,
    pub duration: f64,
    pub at: Instant,
}

impl Timeline {
    /// A timeline at `position` seconds. A duration that isn't finite or is half a second or less gives none.
    pub fn new(position: f64, duration: f64, at: Instant) -> Option<Self> {
        (duration.is_finite() && duration > 0.5 && position.is_finite()).then(|| Self {
            position: position.clamp(0.0, duration),
            duration,
            at,
        })
    }

    /// The position at `now`, moving on only while playing.
    pub fn position_at(&self, now: Instant, playing: bool) -> f64 {
        let moved = if playing { now.saturating_duration_since(self.at).as_secs_f64() } else { 0.0 };
        (self.position + moved).clamp(0.0, self.duration)
    }

    pub fn remaining_at(&self, now: Instant, playing: bool) -> f64 {
        self.duration - self.position_at(now, playing)
    }

    fn agrees_with(&self, other: &Self, playing: bool) -> bool {
        let later = self.at.max(other.at);
        (self.duration - other.duration).abs() < 0.5
            && (self.position_at(later, playing) - other.position_at(later, playing)).abs() < SAME_POSITION
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    pub title: String,
    pub artist: String,
    pub playing: bool,
    /// Changes when the artwork would: the album on a Mac, the app and song on Windows.
    pub artwork_key: String,
    pub timeline: Option<Timeline>,
}

impl Track {
    pub fn is_same_song(&self, other: &Self) -> bool {
        self.title == other.title && self.artist == other.artist && self.artwork_key == other.artwork_key
    }
}

#[derive(Clone, Debug)]
pub struct NowPlaying {
    pub title: String,
    pub artist: String,
    pub artwork: Option<Arc<ColorImage>>,
    pub playing: bool,
    pub timeline: Option<Timeline>,
}

impl NowPlaying {
    /// Whether the UI would show the same thing. Artwork compares by identity, timelines by where they'd be now.
    pub fn same_as(&self, other: &Self) -> bool {
        let same_art = match (&self.artwork, &other.artwork) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        let same_time = match (&self.timeline, &other.timeline) {
            (Some(a), Some(b)) => a.agrees_with(b, self.playing),
            (None, None) => true,
            _ => false,
        };
        self.title == other.title
            && self.artist == other.artist
            && self.playing == other.playing
            && same_art
            && same_time
    }

    pub fn remaining(&self, now: Instant) -> Option<f64> {
        self.timeline.map(|timeline| timeline.remaining_at(now, self.playing))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn song(title: &str, playing: bool) -> NowPlaying {
        NowPlaying { title: title.into(), artist: "Artist".into(), artwork: None, playing, timeline: None }
    }

    #[test]
    fn songs_compare_text_state_and_art() {
        assert!(song("A", true).same_as(&song("A", true)));
        assert!(!song("A", true).same_as(&song("A", false)));
        assert!(!song("A", true).same_as(&song("B", true)));
        let art = Arc::new(ColorImage::filled([2, 2], egui::Color32::RED));
        let with_art = NowPlaying { artwork: Some(art.clone()), ..song("A", true) };
        assert!(with_art.same_as(&NowPlaying { artwork: Some(art), ..song("A", true) }));
        let other_art = Arc::new(ColorImage::filled([2, 2], egui::Color32::RED));
        assert!(!with_art.same_as(&NowPlaying { artwork: Some(other_art), ..song("A", true) }));
        assert!(!with_art.same_as(&song("A", true)));
    }

    #[test]
    fn pause_is_still_the_same_song() {
        let track =
            |playing| Track { title: "A".into(), artist: "B".into(), playing, artwork_key: "k".into(), timeline: None };
        assert!(track(true).is_same_song(&track(false)));
        let other = Track { artwork_key: "j".into(), ..track(true) };
        assert!(!track(true).is_same_song(&other));
    }

    #[test]
    fn timeline_runs_only_while_playing() {
        let t0 = Instant::now();
        let timeline = Timeline::new(60.0, 200.0, t0).expect("valid");
        let later = t0 + Duration::from_secs(10);
        assert_eq!(timeline.position_at(later, true), 70.0);
        assert_eq!(timeline.position_at(later, false), 60.0);
        assert_eq!(timeline.remaining_at(later, true), 130.0);
        assert_eq!(timeline.position_at(t0 + Duration::from_secs(900), true), 200.0);
        assert!(Timeline::new(1.0, 0.0, t0).is_none());
        assert!(Timeline::new(f64::NAN, 10.0, t0).is_none());
    }

    #[test]
    fn steady_timeline_is_not_a_change() {
        let t0 = Instant::now();
        let early = NowPlaying { timeline: Timeline::new(10.0, 200.0, t0), ..song("A", true) };
        let later_at = t0 + Duration::from_secs(5);
        let steady = NowPlaying { timeline: Timeline::new(15.2, 200.0, later_at), ..song("A", true) };
        assert!(early.same_as(&steady));
        let jumped = NowPlaying { timeline: Timeline::new(90.0, 200.0, later_at), ..song("A", true) };
        assert!(!early.same_as(&jumped));
        assert_eq!(early.remaining(t0), Some(190.0));
    }
}
