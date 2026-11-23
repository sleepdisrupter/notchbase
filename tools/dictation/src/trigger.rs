//! The dictation key: hold it to talk, or double-tap it to keep listening (and typing as you
//! talk) until it's pressed again.
//!
//! The key watchers only report what the keyboard did. This decides what that means, so the
//! same rules hold for fn on a Mac and Ctrl+Win on Windows, and the tests can play them out.

use std::time::{Duration, Instant};

/// A press shorter than this is a tap. Two of them keep the microphone on hands-free.
pub const TAP: Duration = Duration::from_millis(320);
/// The second tap of a double tap comes within this long of the first one's release.
pub const DOUBLE: Duration = Duration::from_millis(400);

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
    /// It was a double tap: keep listening until the next press.
    Lock,
    /// Stop and type what was said.
    Finish,
    /// Stop and throw it away.
    Cancel,
    /// Throw away a tap that no second one followed in time, and start listening for this press.
    Restart,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Idle,
    Held(Instant),
    /// Tapped once and let go: a second tap soon keeps it listening, otherwise it's dropped.
    Tapped(Instant),
    /// The second tap, still down.
    Locking,
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
            (State::Held(since), Key::Up) if now.duration_since(since) < self.tap => (State::Tapped(now), Act::Nothing),
            (State::Held(_), Key::Up) => (State::Idle, Act::Finish),
            (State::Held(_), Key::Other) => (State::Idle, Act::Cancel),
            (State::Tapped(up), Key::Down) if now.duration_since(up) < DOUBLE => (State::Locking, Act::Lock),
            (State::Tapped(_), Key::Down) => (State::Held(now), Act::Restart),
            (State::Locking, Key::Up) => (State::Locked, Act::Nothing),
            (State::Locked, Key::Down) => (State::Finishing, Act::Finish),
            (State::Finishing, Key::Up) => (State::Idle, Act::Nothing),
            (State::Held(_) | State::Tapped(_) | State::Locking | State::Locked, Key::Escape) => {
                (State::Idle, Act::Cancel)
            }
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
            State::Held(_) | State::Tapped(_) | State::Locking | State::Locked => {
                self.state = State::Idle;
                Act::Finish
            }
        }
    }

    /// A single tap that no second one followed is dropped: it wasn't dictation.
    pub fn expire(&mut self, now: Instant) -> Act {
        match self.state {
            State::Tapped(up) if now.duration_since(up) >= DOUBLE => {
                self.state = State::Idle;
                Act::Cancel
            }
            _ => Act::Nothing,
        }
    }

    /// Recording ended for another reason, such as an error or the time limit.
    pub fn reset(&mut self) {
        self.state = State::Idle;
    }

    pub fn locked(&self) -> bool {
        matches!(self.state, State::Locked | State::Locking)
    }

    /// Whether the key is still down from the press that started or finished the recording.
    pub fn held(&self) -> bool {
        matches!(self.state, State::Held(_) | State::Locking | State::Finishing)
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

    fn double_tapped() -> Trigger {
        let mut trigger = Trigger::default();
        assert_eq!(trigger.see(Key::Down, at(0)), Act::Start);
        assert_eq!(trigger.see(Key::Up, at(100)), Act::Nothing);
        assert_eq!(trigger.expire(at(200)), Act::Nothing, "the second tap may still come");
        assert_eq!(trigger.see(Key::Down, at(300)), Act::Lock);
        assert!(trigger.locked());
        assert_eq!(trigger.see(Key::Up, at(380)), Act::Nothing);
        trigger
    }

    #[test]
    fn double_tap_keeps_listening_until_next_press() {
        let mut trigger = double_tapped();
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
        let mut trigger = double_tapped();
        assert_eq!(trigger.see(Key::Other, at(2000)), Act::Nothing);
        assert!(trigger.locked());
    }

    #[test]
    fn escape_throws_it_away() {
        let mut trigger = Trigger::default();
        trigger.see(Key::Down, at(0));
        assert_eq!(trigger.see(Key::Escape, at(900)), Act::Cancel);
        assert_eq!(double_tapped().see(Key::Escape, at(3000)), Act::Cancel);
        assert_eq!(Trigger::default().see(Key::Escape, at(0)), Act::Nothing, "Escape alone is left alone");
    }

    #[test]
    fn single_tap_is_dropped() {
        let mut trigger = Trigger::default();
        assert_eq!(trigger.see(Key::Down, at(0)), Act::Start);
        assert_eq!(trigger.see(Key::Up, at(100)), Act::Nothing);
        assert_eq!(trigger.expire(at(100 + DOUBLE.as_millis() as u64)), Act::Cancel);
        assert!(!trigger.locked() && !trigger.held());
        assert_eq!(trigger.see(Key::Down, at(2000)), Act::Start, "ready for the next one");
    }

    #[test]
    fn slow_second_press_is_a_new_hold() {
        let mut trigger = Trigger::default();
        trigger.see(Key::Down, at(0));
        trigger.see(Key::Up, at(100));
        assert_eq!(trigger.see(Key::Down, at(600)), Act::Restart, "the tap is dropped");
        assert!(trigger.held(), "and this press holds");
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
