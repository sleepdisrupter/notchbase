//! What's new: the release notes are announced once after an update, then marked seen.

use std::time::{Duration, Instant};

use winit::window::WindowId;

use crate::bridge::Page;
use crate::config::Config;
use crate::installer::Arrival;
use crate::notch::notice::whats_new::SLOT;
use crate::update::current_version;

use super::preview::previewing;
use super::{App, updates};

const SEEN_AFTER: Duration = Duration::from_millis(1200);
const NOT_SAVED: &str = "Couldn't save the setting";

pub(super) struct News {
    unseen: bool,
    shown_since: Option<Instant>,
}

impl News {
    pub(super) fn at_start(seen: &str, config_reads: bool) -> Self {
        Self::armed(seen, config_reads, previewing())
    }

    fn armed(seen: &str, config_reads: bool, previewing: bool) -> Self {
        Self { unseen: config_reads && !previewing && crate::whats_new::unseen(seen), shown_since: None }
    }

    pub(super) fn unseen(&self) -> bool {
        self.unseen
    }

    fn watched(&mut self, showing: bool, now: Instant) -> bool {
        if !self.unseen || !showing {
            self.shown_since = None;
            return false;
        }
        now.duration_since(*self.shown_since.get_or_insert(now)) >= SEEN_AFTER
    }

    fn wakeup(&self) -> Option<Instant> {
        self.shown_since.filter(|_| self.unseen).map(|since| since + SEEN_AFTER)
    }

    fn seen(&mut self) -> bool {
        self.shown_since = None;
        std::mem::take(&mut self.unseen)
    }
}

pub(super) fn greeting(arrival: Arrival, unseen: bool) -> Option<String> {
    updates::greeting(arrival).filter(|_| !unseen)
}

impl App {
    pub(super) fn announce_changes(&mut self) {
        if !self.news.unseen() {
            return;
        }
        let version = current_version().to_string();
        self.update_notch(|notch| notch.show_updated(&version));
    }

    pub(super) fn follow_whats_new(&mut self, previous: &Config) {
        let seen = &self.config.config().updates.seen;
        if previous.updates.seen != *seen {
            self.news = News::at_start(seen, true);
        }
    }

    pub(super) fn follow_whats_new_page(&mut self, now: Instant) {
        if previewing() || !self.news.unseen() {
            return;
        }
        let showing = self.notches.iter().any(|nw| nw.notch.visible_page() == Some(Page::WhatsNew));
        if self.news.watched(showing, now) {
            self.mark_changes_seen();
        }
    }

    pub(super) fn whats_new_wakeup(&self) -> Option<Instant> {
        self.news.wakeup()
    }

    pub(super) fn view_changes(&mut self, sender: Option<WindowId>) {
        self.mark_changes_seen();
        self.update_notch(|notch| notch.dismiss_notice(SLOT));
        let window = match sender {
            Some(id) => self.notches.window_mut(id),
            None => self.notches.active_mut(),
        };
        if let Some(nw) = window {
            nw.notch.open_page(Page::WhatsNew);
            nw.redraw();
        }
    }

    pub(super) fn mark_changes_seen(&mut self) {
        if !self.news.seen() || previewing() {
            return;
        }
        if let Err(err) = self.config.update(|config| config.updates.seen = current_version().to_string()) {
            self.report(NOT_SAVED, err);
        }
        self.redraw_notch();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUNNING: &str = env!("CARGO_PKG_VERSION");
    const FRAME: Duration = Duration::from_millis(16);
    const ARRIVALS: [Arrival; 3] = [Arrival::Fresh, Arrival::Updated, Arrival::Installed];

    fn unseen() -> News {
        News::armed("0.13.0", true, false)
    }

    #[test]
    fn news_only_if_unseen() {
        assert!(News::armed("", true, false).unseen());
        assert!(News::armed("0.13.0", true, false).unseen());
        assert!(!News::armed(RUNNING, true, false).unseen());
        assert!(!News::armed("0.13.0", false, false).unseen(), "a config with a mistake cannot remember seeing them");
        assert!(!News::armed("0.13.0", true, true).unseen(), "a preview shows samples and never marks");
    }

    #[test]
    fn unseen_notes_replace_greeting() {
        for arrival in ARRIVALS {
            assert_eq!(greeting(arrival, true), None, "{arrival:?}: the notice with View Changes says it instead");
        }
    }

    #[test]
    fn seen_notes_keep_greeting() {
        for arrival in ARRIVALS {
            assert_eq!(greeting(arrival, false), updates::greeting(arrival), "{arrival:?}");
        }
        assert!(greeting(Arrival::Updated, false).is_some_and(|said| said.starts_with("Updated to notchbase ")));
        assert!(greeting(Arrival::Installed, false).is_some_and(|said| said.starts_with("notchbase is installed")));
        assert_eq!(greeting(Arrival::Fresh, false), None);
    }

    #[test]
    fn seen_after_a_moment() {
        let (mut news, t0) = (unseen(), Instant::now());
        assert!(!news.watched(true, t0));
        assert_eq!(news.wakeup(), Some(t0 + SEEN_AFTER));
        assert!(!news.watched(true, t0 + SEEN_AFTER - FRAME));
        assert!(news.watched(true, t0 + SEEN_AFTER));
        assert!(news.seen());
        assert_eq!((news.unseen(), news.wakeup()), (false, None));
        assert!(!news.seen(), "marking twice changes nothing");
    }

    #[test]
    fn glimpse_does_not_count() {
        let (mut news, t0) = (unseen(), Instant::now());
        assert!(!news.watched(true, t0));
        assert!(!news.watched(false, t0 + FRAME));
        assert_eq!(news.wakeup(), None);
        let back = t0 + SEEN_AFTER * 3;
        assert!(!news.watched(true, back), "time spent elsewhere is not time spent reading");
        assert!(!news.watched(true, back + SEEN_AFTER - FRAME));
        assert!(news.watched(true, back + SEEN_AFTER));
        assert!(news.unseen(), "watching alone never marks");
    }

    #[test]
    fn stops_watching_once_seen() {
        let (mut news, t0) = (News::armed(RUNNING, true, false), Instant::now());
        assert!(!news.watched(true, t0));
        assert!(!news.watched(true, t0 + SEEN_AFTER * 2));
        assert_eq!(news.wakeup(), None);
        assert!(!news.seen());
    }
}
