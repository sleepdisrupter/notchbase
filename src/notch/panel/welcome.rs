//! Opening onto the welcome walkthrough while it is still due.

use crate::bridge::Page;
use crate::notch::{Mode, Notch};

impl Notch {
    pub(super) fn opens_to_welcome(&mut self) -> bool {
        if !self.welcome_due || !matches!(self.mode, Mode::Idle | Mode::Peek | Mode::Activity) {
            return false;
        }
        self.pager.show(Page::Welcome);
        true
    }

    pub(super) fn leave_welcome_when_done(&mut self) {
        let Some(to_settings) = self.pages.take_welcome_done() else { return };
        self.switch_to(if to_settings { Page::Settings } else { self.way_back() });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notch::tests::fixtures::island;

    #[test]
    fn due_walkthrough_opens_first() {
        for mode in [Mode::Idle, Mode::Peek, Mode::Activity] {
            let mut n = island();
            (n.welcome_due, n.news_due, n.mode) = (true, true, mode);
            n.opens_to_start();
            assert_eq!(n.pager.page, Page::Welcome, "{mode:?}: it comes before What's New");
        }
    }

    #[test]
    fn open_panel_stays_put() {
        for mode in [Mode::Expanded, Mode::Drop, Mode::Ask] {
            let mut n = island();
            (n.welcome_due, n.mode) = (true, mode);
            assert!(!n.opens_to_welcome(), "{mode:?}");
            assert_ne!(n.pager.page, Page::Welcome);
        }
        let mut done = island();
        assert!(!done.opens_to_welcome());
    }

    #[test]
    fn walkthrough_holds_panel_open() {
        let n = island();
        assert!(n.pages.holds_open(Page::Welcome));
    }
}
