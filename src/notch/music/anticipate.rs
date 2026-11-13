//! Play, pause and skip answer at once, before the player reports back, and a stale report can't undo them.

use std::time::{Duration, Instant};

use crate::media::{Control, NowPlaying};

use super::super::Notch;
use super::Music;
use super::pulse::{Beat, Pulse};

const TRUST_FOR: Duration = Duration::from_millis(1500);

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Expectation {
    playing: bool,
    until: Instant,
}

impl Music {
    pub(super) fn believed(&mut self, reported: &NowPlaying, now: Instant) -> NowPlaying {
        match self.expecting {
            Some(expected) if now < expected.until && reported.title == self.now.title => {
                if reported.playing == expected.playing {
                    self.expecting = None;
                    reported.clone()
                } else {
                    NowPlaying { playing: expected.playing, ..reported.clone() }
                }
            }
            _ => {
                self.expecting = None;
                reported.clone()
            }
        }
    }
}

impl Notch {
    pub fn anticipate(&mut self, control: Control) {
        let now = Instant::now();
        let Some(music) = &mut self.music else { return };
        match control {
            Control::PlayPause => {
                let playing = !music.now.playing;
                music.now.playing = playing;
                music.expecting = Some(Expectation { playing, until: now + TRUST_FOR });
                music.pulse = Some(Pulse::new(if playing { Beat::Play } else { Beat::Pause }, now));
            }
            Control::Next | Control::Previous => {
                let direction = if control == Control::Next { 1.0 } else { -1.0 };
                music.skip_direction = direction;
                music.pulse = Some(Pulse::new(Beat::Nudge(direction), now));
            }
            Control::Seek(_) => return,
        }
        self.reveal_music_now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notch::tests::fixtures::{island, song};

    #[test]
    fn play_pause_answers_at_once() {
        let mut n = island();
        n.set_now_playing(Some(&song(true, 10.0)));
        n.anticipate(Control::PlayPause);
        let music = n.music.as_ref().expect("music");
        assert!(!music.now.playing, "paused before the player says so");
        assert!(music.pulse.is_some());
        let pulse = music.pulse;
        assert!(!n.set_now_playing(Some(&song(true, 10.0))), "a late 'still playing' report is ignored");
        assert_eq!(n.music.as_ref().and_then(|music| music.pulse), pulse, "no second pulse");
        n.set_now_playing(Some(&song(false, 10.0)));
        assert!(n.music.as_ref().is_some_and(|music| music.expecting.is_none()), "confirmed");
    }

    #[test]
    fn skip_nudges_and_slides_in() {
        let mut n = island();
        n.set_now_playing(Some(&song(true, 10.0)));
        n.anticipate(Control::Previous);
        assert!(n.music.as_ref().is_some_and(|music| music.pulse.is_some() && music.skip_direction == -1.0));
        let next = NowPlaying { title: "Earlier".into(), ..song(true, 0.0) };
        n.set_now_playing(Some(&next));
        assert_eq!(n.music.as_ref().map(|music| music.skip_direction), Some(1.0));
    }
}
