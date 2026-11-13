//! Wakes the notch once a second while a song plays, so the time left stays right.

use std::time::{Duration, Instant};

use super::super::Notch;

impl Notch {
    pub(in crate::notch) fn music_tick(&mut self, now: Instant) -> bool {
        let Some(music) = &mut self.music else { return false };
        let settled = music.cover.tick(now);
        if settled {
            music.retint(now);
        }
        let second = music.remaining(now).map(|left| left.ceil() as u64);
        let changed = second != music.second || settled;
        music.second = second;
        changed && self.shows_music()
    }

    pub(in crate::notch) fn music_wakeup(&self, now: Instant) -> Option<Instant> {
        let due = self.music.as_ref().and_then(|music| music.cover.wakeup());
        due.into_iter().chain(self.next_second(now)).min()
    }

    fn next_second(&self, now: Instant) -> Option<Instant> {
        let music = self.music.as_ref().filter(|music| music.now.playing && self.shows_music())?;
        let left = music.remaining(now)?;
        let until_tick = match left.fract() {
            fraction if fraction > 0.001 => fraction,
            _ => 1.0,
        };
        (left > 0.0).then(|| now + Duration::from_secs_f64(until_tick))
    }
}
