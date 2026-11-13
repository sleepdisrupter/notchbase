//! Whether the question card has the notch.

use super::super::{Mode, Notch};
use crate::notch::moves::transition::Motion;

impl Notch {
    /// The question the card shows, while it has the notch.
    pub(in crate::notch) fn front(&self) -> Option<u64> {
        self.ask.as_ref().filter(|_| self.card).map(|ask| ask.id)
    }

    pub(in crate::notch) fn follow_card(&mut self, leave: Motion) {
        let before = self.front();
        self.card = !self.locked && self.ask.is_some();
        let switched = before != self.front();
        match (self.card, self.mode) {
            (true, Mode::Drop) | (false, Mode::Idle | Mode::Activity | Mode::Peek | Mode::Expanded | Mode::Drop) => {}
            (true, Mode::Ask) if switched => {
                self.ask_fade.snap_to(0.0);
                self.retarget(Motion::Resize);
            }
            (true, Mode::Ask) => {}
            (true, _) => self.set_mode(Mode::Ask, Motion::Pop),
            (false, Mode::Ask) => self.return_to_rest_by(leave),
        }
    }
}
