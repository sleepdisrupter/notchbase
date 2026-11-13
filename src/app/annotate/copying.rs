//! Copies the annotated picture to the clipboard once the hand rests.

use std::time::{Duration, Instant};

const COPY_AFTER: Duration = Duration::from_millis(250);

#[derive(Default)]
pub(super) struct Copying {
    due: Option<Instant>,
    drawn: Option<u64>,
    on_clipboard: Option<u64>,
    refused: Option<u64>,
}

impl Copying {
    pub(super) fn drew(&mut self, revision: u64, now: Instant, at_the_keyboard: bool) {
        self.drawn = Some(revision);
        self.due = Some(if at_the_keyboard { now + COPY_AFTER } else { now });
    }

    pub(super) fn due(&self) -> Option<Instant> {
        self.due
    }

    pub(super) fn take_due(&mut self, now: Instant, at_the_keyboard: bool) -> bool {
        self.due.is_some_and(|at| !at_the_keyboard || at <= now) && self.take()
    }

    pub(super) fn take(&mut self) -> bool {
        self.due.take().is_some()
    }

    pub(super) fn copied(&mut self, revision: u64) {
        self.on_clipboard = self.on_clipboard.max(Some(revision));
    }

    pub(super) fn refused(&mut self, revision: u64) {
        self.refused = self.refused.max(Some(revision));
    }

    pub(super) fn awaited(&self) -> Option<u64> {
        self.drawn.filter(|_| self.drawn > self.on_clipboard.max(self.refused))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOON: Duration = Duration::from_millis(100);
    const AT_THE_KEYBOARD: bool = true;
    const KEYBOARD_ELSEWHERE: bool = false;

    #[test]
    fn nothing_copied_before_drawing() {
        let mut copying = Copying::default();
        assert_eq!(copying.due(), None);
        assert!(!copying.take_due(Instant::now() + COPY_AFTER, AT_THE_KEYBOARD) && !copying.take());
        assert!(!copying.take_due(Instant::now(), KEYBOARD_ELSEWHERE), "nor when the keyboard leaves");
        assert_eq!(copying.awaited(), None);
    }

    #[test]
    fn changes_push_the_copy_back() {
        let start = Instant::now();
        let mut copying = Copying::default();
        copying.drew(1, start, AT_THE_KEYBOARD);
        copying.drew(2, start + SOON, AT_THE_KEYBOARD);
        assert_eq!(copying.due(), Some(start + SOON + COPY_AFTER));
        assert!(!copying.take_due(start + COPY_AFTER, AT_THE_KEYBOARD), "the first stroke's deadline was replaced");
        assert!(copying.take_due(start + SOON + COPY_AFTER, AT_THE_KEYBOARD));
        assert_eq!(copying.due(), None);
        let later = start + SOON + COPY_AFTER * 2;
        assert!(!copying.take_due(later, AT_THE_KEYBOARD), "one rest hands the picture over once");
    }

    #[test]
    fn change_while_away_copies_at_once() {
        let start = Instant::now();
        let mut copying = Copying::default();
        copying.drew(1, start, KEYBOARD_ELSEWHERE);
        assert_eq!(copying.due(), Some(start));
        assert!(copying.take_due(start, KEYBOARD_ELSEWHERE), "whoever took the keyboard may paste straight away");
        assert_eq!(copying.awaited(), Some(1));
        copying.drew(2, start + SOON, AT_THE_KEYBOARD);
        assert!(!copying.take_due(start + SOON, AT_THE_KEYBOARD), "back at the keyboard the hand is waited for again");
        assert!(copying.take_due(start + SOON + COPY_AFTER, AT_THE_KEYBOARD));
    }

    #[test]
    fn waiting_copy_goes_when_keys_leave() {
        let start = Instant::now();
        let mut copying = Copying::default();
        copying.drew(1, start, AT_THE_KEYBOARD);
        assert!(!copying.take_due(start + SOON, AT_THE_KEYBOARD), "the hand has not rested yet");
        assert!(copying.take_due(start + SOON, KEYBOARD_ELSEWHERE), "but the keyboard has gone, so it is not kept");
        assert_eq!((copying.due(), copying.awaited()), (None, Some(1)));
        assert!(!copying.take_due(start + COPY_AFTER, KEYBOARD_ELSEWHERE), "and it is handed over once");
    }

    #[test]
    fn closing_copies_at_once() {
        let start = Instant::now();
        let mut copying = Copying::default();
        copying.drew(1, start, AT_THE_KEYBOARD);
        assert!(copying.take());
        assert!(!copying.take() && copying.due().is_none());
    }

    #[test]
    fn waits_for_latest_picture() {
        let start = Instant::now();
        let mut copying = Copying::default();
        copying.drew(3, start, AT_THE_KEYBOARD);
        assert_eq!(copying.awaited(), Some(3));
        copying.copied(2);
        assert_eq!(copying.awaited(), Some(3), "an older picture landed");
        copying.copied(3);
        assert_eq!(copying.awaited(), None);
        copying.copied(1);
        assert_eq!(copying.awaited(), None, "a late report never moves the clipboard backwards");
        copying.drew(4, start + SOON, AT_THE_KEYBOARD);
        assert_eq!(copying.awaited(), Some(4));
    }

    #[test]
    fn refused_picture_not_awaited() {
        let start = Instant::now();
        let mut copying = Copying::default();
        copying.drew(3, start, AT_THE_KEYBOARD);
        copying.refused(2);
        assert_eq!(copying.awaited(), Some(3), "an older picture was refused");
        copying.refused(3);
        assert_eq!(copying.awaited(), None);
        copying.drew(4, start + SOON, AT_THE_KEYBOARD);
        assert_eq!(copying.awaited(), Some(4));
    }
}
