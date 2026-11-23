//! The dictation key: hold it to talk, or tap it to keep listening until it's tapped again.
//!
//! The key watchers only report what the keyboard did. This decides what that means, so the
//! same rules hold for fn on a Mac and Ctrl+Win on Windows, and the tests can play them out.

use std::time::{Duration, Instant};

/// A press shorter than this is a tap, which keeps the microphone on hands-free.
pub const TAP: Duration = Duration::from_millis(320);

/// What the keyboard did, as the watchers report it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// The dictation key (or the last key of its chord) went down.
    Down,
    /// The dictation key, or a key of its chord, came up.
    Up,
    /// Another key went down while the dictation key was held: it was a shortcut, not dictation.
    Other,
    /// Escape, which throws the recording away.
    Escape,
}

/// What the engine should do about it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    Nothing,
    /// Start listening, held for now.
    Start,
    /// It was a tap: keep listening until the next press.
    Lock,
    /// Stop and type what was said.
    Finish,
    /// Stop and throw it away.
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Idle,
    Held(Instant),
    Locked,
    /// Pressed again to finish a hands-free recording, waiting for that press to end.
    Finishing,
}

#[derive(Debug)]
pub struct Trigger {
    state: State,
    tap: Duration,
}

impl Default for Trigger {
    fn default() -> Self {
        Self { state: State::Idle, tap: TAP }
    }
}

impl Trigger {
    pub fn see(&mut self, key: Key, now: Instant) -> Act {
        let (state, act) = match (self.state, key) {
            (State::Idle, Key::Down) => (State::Held(now), Act::Start),
            (State::Held(since), Key::Up) if now.duration_since(since) < self.tap => (State::Locked, Act::Lock),
            (State::Held(_), Key::Up) => (State::Idle, Act::Finish),
            (State::Held(_), Key::Other) => (State::Idle, Act::Cancel),
            (State::Locked, Key::Down) => (State::Finishing, Act::Finish),
            (State::Finishing, Key::Up) => (State::Idle, Act::Nothing),
            (State::Held(_) | State::Locked, Key::Escape) => (State::Idle, Act::Cancel),
            (state, _) => (state, Act::Nothing),
        };
        self.state = state;
        act
    }

    /// The tile, the shortcut or the live activity's button: start hands-free, or finish.
    pub fn toggle(&mut self) -> Act {
        match self.state {
            State::Idle | State::Finishing => {
                self.state = State::Locked;
                Act::Start
            }
            State::Held(_) | State::Locked => {
                self.state = State::Idle;
                Act::Finish
            }
        }
    }

    /// Recording ended for another reason, such as an error or the time limit.
    pub fn reset(&mut self) {
        self.state = State::Idle;
    }

    pub fn locked(&self) -> bool {
        self.state == State::Locked
    }

    /// Whether the key is still down from the press that started or finished the recording.
    pub fn held(&self) -> bool {
        matches!(self.state, State::Held(_) | State::Finishing)
    }

    /// The watcher saw the key is up although no release came through (a tap that was switched
    /// off for a moment, say). Treat it as the release.
    pub fn released(&mut self, now: Instant) -> Act {
        if self.held() { self.see(Key::Up, now) } else { Act::Nothing }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(ms: u64) -> Instant {
        thread_local!(static ZERO: Instant = Instant::now());
        ZERO.with(|zero| *zero + Duration::from_millis(ms))
    }

    #[test]
    fn hold_talks_until_release() {
        let mut trigger = Trigger::default();
        assert_eq!(trigger.see(Key::Down, at(0)), Act::Start);
        assert!(trigger.held() && !trigger.locked());
        assert_eq!(trigger.see(Key::Up, at(1500)), Act::Finish);
        assert!(!trigger.held() && !trigger.locked());
    }

    #[test]
    fn tap_keeps_listening_until_next_press() {
        let mut trigger = Trigger::default();
        assert_eq!(trigger.see(Key::Down, at(0)), Act::Start);
        assert_eq!(trigger.see(Key::Up, at(150)), Act::Lock);
        assert!(trigger.locked());
        assert_eq!(trigger.see(Key::Down, at(9000)), Act::Finish);
        assert!(!trigger.locked());
        assert!(trigger.held(), "the finishing press is still down");
        assert_eq!(trigger.see(Key::Up, at(9100)), Act::Nothing);
        assert!(!trigger.held());
        assert_eq!(trigger.see(Key::Down, at(9500)), Act::Start, "ready for the next one");
    }

    #[test]
    fn shortcut_with_the_key_cancels() {
        let mut trigger = Trigger::default();
        trigger.see(Key::Down, at(0));
        assert_eq!(trigger.see(Key::Other, at(80)), Act::Cancel);
        assert_eq!(trigger.see(Key::Up, at(200)), Act::Nothing, "its release isn't a tap");
        assert!(!trigger.held() && !trigger.locked());
    }

    #[test]
    fn typing_while_hands_free_is_fine() {
        let mut trigger = Trigger::default();
        trigger.see(Key::Down, at(0));
        trigger.see(Key::Up, at(100));
        assert_eq!(trigger.see(Key::Other, at(2000)), Act::Nothing);
        assert!(trigger.locked());
    }

    #[test]
    fn escape_throws_it_away() {
        let mut trigger = Trigger::default();
        trigger.see(Key::Down, at(0));
        assert_eq!(trigger.see(Key::Escape, at(900)), Act::Cancel);
        let mut locked = Trigger::default();
        locked.see(Key::Down, at(0));
        locked.see(Key::Up, at(100));
        assert_eq!(locked.see(Key::Escape, at(3000)), Act::Cancel);
        assert_eq!(Trigger::default().see(Key::Escape, at(0)), Act::Nothing, "Escape alone is left alone");
    }

    #[test]
    fn toggle_starts_hands_free_and_finishes() {
        let mut trigger = Trigger::default();
        assert_eq!(trigger.toggle(), Act::Start);
        assert!(trigger.locked());
        assert_eq!(trigger.toggle(), Act::Finish);
        assert!(!trigger.locked() && !trigger.held());
    }

    #[test]
    fn missed_release_still_finishes() {
        let mut trigger = Trigger::default();
        trigger.see(Key::Down, at(0));
        assert_eq!(trigger.released(at(2000)), Act::Finish);
        assert_eq!(trigger.released(at(2100)), Act::Nothing);
    }
}
