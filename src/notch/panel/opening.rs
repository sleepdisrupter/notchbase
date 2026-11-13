//! Which page the panel opens on.

use crate::bridge::{Activity, Page};
use crate::notch::{Mode, Notch};

impl Notch {
    pub(in crate::notch) fn opens_on_click(&mut self) {
        let page = self.live.as_ref().filter(|_| self.shows_live()).and_then(|live| match live.activity {
            Activity::Agent(_) => Some(Page::Agents),
            Activity::Video(_) => Some(Page::Shelf),
            Activity::Tool(..) | Activity::Lock(_) => None,
        });
        match page {
            Some(page) if self.has_tab(page) && matches!(self.mode, Mode::Idle | Mode::Peek) => self.pager.show(page),
            _ => self.opens_to_start(),
        }
    }

    pub(in crate::notch) fn has_tab(&self, page: Page) -> bool {
        self.tabs.iter().any(|tab| tab.page == page)
    }

    pub(in crate::notch) fn opens_to_start(&mut self) {
        if self.opens_to_welcome() || self.opens_to_news() || !matches!(self.mode, Mode::Idle | Mode::Peek) {
            return;
        }
        if self.shelf_filled && self.has_tab(Page::Shelf) {
            self.pager.show(Page::Shelf);
        } else {
            self.opens_to_music();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::notch::moves::surge::SURGE;
    use crate::notch::tests::fixtures::{OVER, island, peek, song};

    #[test]
    fn filled_shelf_opens_first() {
        let mut n = island();
        n.set_now_playing(Some(&song(true, 0.0)));
        n.shelf_filled = true;
        n.opens_to_start();
        assert_eq!(n.pager.page, Page::Shelf, "even over a playing song");
    }

    #[test]
    fn empty_shelf_leaves_player_first() {
        let mut n = island();
        n.set_now_playing(Some(&song(true, 0.0)));
        n.opens_to_start();
        assert_eq!(n.pager.page, Page::Media);
    }

    #[test]
    fn busy_assistant_opens_agents() {
        let mut n = island();
        n.set_activities(vec![crate::notch::live::tests::claude()]);
        n.mode = Mode::Peek;
        n.opens_on_click();
        assert_eq!(n.pager.page, Page::Agents);
        let mut quiet = island();
        quiet.mode = Mode::Peek;
        quiet.opens_on_click();
        assert_ne!(quiet.pager.page, Page::Agents, "without a live run it opens as usual");
        let mut hidden = island();
        hidden.tabs.retain(|tab| tab.page != Page::Agents);
        hidden.set_activities(vec![crate::notch::live::tests::claude()]);
        hidden.mode = Mode::Peek;
        hidden.opens_on_click();
        assert_ne!(hidden.pager.page, Page::Agents, "no Agents tab, no Agents page");
    }

    #[test]
    fn holding_still_opens_shelf() {
        let mut n = island();
        n.shelf_filled = true;
        let t0 = peek(&mut n, Instant::now(), OVER);
        let delay = crate::config::current().notch.hover_delay();
        n.tick(t0 + delay + SURGE + std::time::Duration::from_millis(10), Some(OVER));
        assert_eq!((n.mode, n.pager.page), (Mode::Expanded, Page::Shelf));
    }
}
