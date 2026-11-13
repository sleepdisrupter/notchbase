//! The "Updated to notchbase x.y" notice, which leads to the release notes.

use std::time::Duration;

use notchbase_sdk::element::lower_static;
use notchbase_sdk::prelude::*;
use notchbase_sdk::protocol::NoticeNode;

use crate::bridge::{Action, Page, ToolSlot};
use crate::notch::activity::ActivityKind;
use crate::notch::{Mode, Notch};

pub const SLOT: ToolSlot = ToolSlot(u16::MAX - 3);
const SHOWN_FOR: Duration = Duration::from_secs(8);
const BADGE: f32 = 18.0;
const UPDATED: &str = "Updated";
const PRODUCT: &str = "notchbase";
const VIEW_CHANGES: &str = "View Changes";
const CHANGES: &str = "Changes";

pub fn updated(version: &str) -> NoticeNode {
    let island = text(format!("{UPDATED} to {PRODUCT} {version}")).callout();
    let words = adaptive(text(UPDATED).callout(), island);
    let spark = badge(Icon::Spark, Tone::Blue, BADGE);
    let shown = Notice::new(spark, words).pop().size(NoticeSize::Large).duration(SHOWN_FOR).into_node();
    let view = adaptive(button(CHANGES).on_press(()), button(VIEW_CHANGES).on_press(()));
    NoticeNode { controls: Some(lower_static(view)), ..shown }
}

pub(in crate::notch) fn pressed(slot: ToolSlot, presses: Vec<Action>) -> Vec<Action> {
    if slot == SLOT && !presses.is_empty() { vec![Action::ViewChanges] } else { presses }
}

impl Notch {
    pub fn show_updated(&mut self, version: &str) {
        if !self.says_something() {
            self.show_notice(SLOT, updated(version));
        }
    }

    fn says_something(&self) -> bool {
        self.activity.is_live() && matches!(self.activity.kind, Some(ActivityKind::Message(_)))
    }

    pub(in crate::notch) fn reads_news(&self) -> bool {
        self.news_due && self.pager.page == Page::WhatsNew
    }

    pub(in crate::notch) fn walked_away_from_news(&self, reading: bool) -> Option<Action> {
        (reading && self.pager.page != Page::WhatsNew).then_some(Action::ChangesSeen)
    }

    pub(in crate::notch) fn opens_to_news(&mut self) -> bool {
        if !self.news_due || !matches!(self.mode, Mode::Idle | Mode::Peek | Mode::Activity) {
            return false;
        }
        self.dismiss_notice(SLOT);
        self.pager.show(Page::WhatsNew);
        true
    }
}

#[cfg(test)]
mod tests {
    use notchbase_sdk::protocol::{HandlerId, Node, Surface, UiValue};

    use super::*;
    use crate::notch::tests::fixtures::{island, sample_notice};

    const OTHER: ToolSlot = ToolSlot(1);
    const PROBLEM: &str = "Screenshot shortcut is unavailable";

    fn shows_updated(n: &Notch) -> bool {
        n.activity.is_live() && matches!(&n.activity.kind, Some(ActivityKind::Tool(notice)) if notice.slot == SLOT)
    }

    fn press(slot: ToolSlot) -> Action {
        Action::Tool(slot, Surface::Notice, HandlerId(0), UiValue::Press)
    }

    fn label(node: &Node) -> Option<&str> {
        match node {
            Node::Button { label, on_press: Some(_), .. } => Some(label),
            _ => None,
        }
    }

    #[test]
    fn names_version_with_one_button() {
        let notice = updated("9.8.7");
        let Some(Node::Adaptive { notch, island }) = notice.controls.as_ref() else {
            panic!("a button for each style")
        };
        assert_eq!(label(notch), Some(CHANGES), "short enough to sit beside a camera");
        assert_eq!(label(island), Some(VIEW_CHANGES));
        assert!(format!("{:?}", notice.trailing).contains("Updated to notchbase 9.8.7"));
        assert!(notice.below.is_none() && !notice.banner, "anything under it would stop an island tucking the button");
    }

    #[test]
    fn leaves_so_clicks_still_open() {
        let notice = updated("9.8.7");
        assert!(!notice.sticky());
        assert_eq!(Duration::from_millis(notice.duration_ms), SHOWN_FOR);
        assert!(notice.pop && !notice.shake);
    }

    #[test]
    fn press_asks_for_changes() {
        assert_eq!(pressed(SLOT, vec![press(SLOT)]), [Action::ViewChanges]);
        assert_eq!(pressed(OTHER, vec![press(OTHER)]), [press(OTHER)]);
        assert_eq!(pressed(SLOT, Vec::new()), []);
    }

    #[test]
    fn news_opens_from_rest_peek_notice() {
        for mode in [Mode::Idle, Mode::Peek, Mode::Activity] {
            let mut n = island();
            (n.news_due, n.mode) = (true, mode);
            assert!(n.opens_to_news(), "{mode:?}");
            assert_eq!(n.pager.page, Page::WhatsNew);
        }
        for mode in [Mode::Expanded, Mode::Drop, Mode::Ask] {
            let mut n = island();
            (n.news_due, n.mode) = (true, mode);
            assert!(!n.opens_to_news(), "{mode:?}");
            assert_ne!(n.pager.page, Page::WhatsNew);
        }
        let mut seen = island();
        assert!(!seen.opens_to_news());
        assert_ne!(seen.pager.page, Page::WhatsNew);
    }

    #[test]
    fn news_ends_only_its_notice() {
        let now = std::time::Instant::now();
        let mut n = island();
        n.news_due = true;
        n.show_notice(SLOT, updated("9.8.7"));
        assert!(n.activity.is_live());
        assert!(n.opens_to_news());
        assert!(!n.activity.is_live(), "or it would come back when the panel closes");
        let mut other = island();
        other.news_due = true;
        other.start_activity(ActivityKind::Tool(sample_notice(2_000)), now);
        assert!(other.opens_to_news());
        assert!(other.activity.is_live());
    }

    #[test]
    fn keeps_startup_message() {
        let mut told = island();
        told.show_message(PROBLEM);
        told.show_updated("9.8.7");
        assert!(told.activity.is_live());
        assert_eq!(told.activity.kind, Some(ActivityKind::Message(PROBLEM.to_owned())), "the problem stays readable");
        told.news_due = true;
        assert!(told.opens_to_news(), "the open onto the page still delivers the notes");
        assert_eq!(told.pager.page, Page::WhatsNew);
    }

    #[test]
    fn shows_over_quiet_or_ended() {
        let mut quiet = island();
        quiet.show_updated("9.8.7");
        assert!(shows_updated(&quiet));
        let mut busy = island();
        busy.start_activity(ActivityKind::Tool(sample_notice(2_000)), std::time::Instant::now());
        busy.show_updated("9.8.7");
        assert!(shows_updated(&busy), "a tool's own notice at start gives way, as any notice does");
        let mut past = island();
        past.show_message(PROBLEM);
        past.end_activity();
        past.show_updated("9.8.7");
        assert!(shows_updated(&past));
    }
}
