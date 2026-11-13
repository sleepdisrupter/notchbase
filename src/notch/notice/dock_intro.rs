//! The one-time notice that introduces notchbase dock, with Turn On, Not Now and a chevron that opens it into a card:
//! a picture of the dock, a short overview and the same two buttons. The dock is off until someone turns it on: Turn On
//! opens Settings › Dock, which says what macOS will ask for before the switch starts it.

use std::time::Duration;

use notchbase_sdk::element::lower_static;
use notchbase_sdk::prelude::*;
use notchbase_sdk::protocol::{HandlerId, ImageSource, NoticeNode, UiValue};

use crate::bridge::{Action, Page, ToolSlot};
use crate::notch::Notch;
use crate::notch::activity::ActivityKind;

pub const SLOT: ToolSlot = ToolSlot(u16::MAX - 7);
const SHOWN_FOR: Duration = Duration::from_secs(12);
/// Long enough to read the card; a choice or the chevron ends it sooner.
const OPEN_FOR: Duration = Duration::from_secs(45);
const PICTURE: [f32; 2] = [296.0, 77.0];
const PICTURE_RADIUS: f32 = 12.0;
const LINE_GAP: f32 = 6.0;
const PICTURE_GAP: f32 = 12.0;
const BADGE: f32 = 18.0;
const BUTTON_GAP: f32 = 6.0;
const SHORT: &str = "New Dock";
const WHOLE: &str = "Meet notchbase dock";
const TURN_ON: &str = "Turn On";
const NOT_NOW: &str = "Not Now";
const TURN_ON_PRESS: HandlerId = HandlerId(0);
const NOT_NOW_PRESS: HandlerId = HandlerId(1);
const CHEVRON_PRESS: HandlerId = HandlerId(2);
#[cfg(not(windows))]
const OVERVIEW: [&str; 4] = [
    "Replaces the macOS Dock with the notch's black glass",
    "Liquid open, minimize and remove animations",
    "Live window previews and its own Cmd+Tab",
    "Off until you turn it on, any time in Settings \u{203a} Dock",
];
#[cfg(windows)]
const OVERVIEW: [&str; 4] = [
    "Replaces the taskbar with the notch's black glass",
    "Liquid open, minimize and remove animations",
    "Live window previews and a Downloads stack",
    "Off until you turn it on, any time in Settings \u{203a} Dock",
];

/// Shown once: short beside a camera, whole on an island, Turn On and Not Now side by side, and a chevron to open it.
pub fn intro() -> NoticeNode {
    let words = adaptive(text(SHORT).callout(), text(WHOLE).callout());
    let grid = badge(Icon::Grid, Tone::Blue, BADGE);
    let shown = Notice::new(grid, words).pop().size(NoticeSize::Large).duration(SHOWN_FOR).into_node();
    NoticeNode { controls: Some(buttons(Icon::ChevronDown)), ..shown }
}

/// The notice opened into a card: the picture, the overview and the same buttons, with the chevron to close it again.
pub fn opened() -> NoticeNode {
    let grid = badge(Icon::Grid, Tone::Blue, BADGE);
    let picture =
        image(ImageSource::File(crate::tools::DOCK_PICTURE.into())).size(PICTURE[0], PICTURE[1]).radius(PICTURE_RADIUS);
    let line = |words: &str| text(format!("\u{2022}  {words}")).subheadline().secondary().wrap();
    let [a, b, c, d] = OVERVIEW;
    let lines = notchbase_sdk::column![line(a), line(b), line(c), line(d)].spacing(LINE_GAP);
    let card = notchbase_sdk::column![picture, lines].spacing(PICTURE_GAP);
    let shown = Notice::new(grid, text(WHOLE).headline()).size(NoticeSize::Large).duration(OPEN_FOR).into_node();
    NoticeNode { below: Some(lower_static(card)), controls: Some(buttons(Icon::ChevronUp)), ..shown }
}

/// Turn On and Not Now, then the chevron that opens or closes the card.
fn buttons(chevron: Icon) -> notchbase_sdk::protocol::Node {
    let turn = button(TURN_ON).on_press(());
    let later = button(NOT_NOW).on_press(());
    lower_static(row![turn, later, button("").icon(chevron).on_press(())].spacing(BUTTON_GAP))
}

pub(in crate::notch) fn pressed(slot: ToolSlot, presses: Vec<Action>) -> Vec<Action> {
    if slot != SLOT {
        return presses;
    }
    let answer = |press| match press {
        Action::Tool(_, _, TURN_ON_PRESS, UiValue::Press) => Some(Action::AnswerDockIntro(true)),
        Action::Tool(_, _, NOT_NOW_PRESS, UiValue::Press) => Some(Action::AnswerDockIntro(false)),
        Action::Tool(_, _, CHEVRON_PRESS, UiValue::Press) => Some(Action::OpenDockIntro),
        _ => None,
    };
    presses.into_iter().filter_map(answer).collect()
}

/// Whether `node` is the card rather than the compact notice.
pub fn is_open(node: &NoticeNode) -> bool {
    node.below.is_some()
}

impl Notch {
    pub fn introduce_dock(&mut self) {
        self.show_notice(SLOT, intro());
    }

    pub fn close_dock_intro(&mut self) {
        self.dismiss_notice(SLOT);
    }

    /// The chevron: opens the notice into its card, or folds the card back, in place with the notch's spring. Nothing
    /// happens once the notice has gone.
    pub fn toggle_dock_intro(&mut self) -> bool {
        let Some(ActivityKind::Tool(notice)) = &self.activity.kind else { return false };
        if notice.slot != SLOT || !self.activity.is_live() {
            return false;
        }
        let next = if is_open(&notice.node) { intro() } else { opened() };
        self.reshape_notice(SLOT, next)
    }

    /// Settings › Dock, where the switch and what it needs are.
    pub fn open_dock_settings(&mut self) {
        self.dismiss_notice(SLOT);
        self.open_page(Page::Settings);
        self.pages.show_dock_settings();
    }

    /// Whether a notice of any kind is up, so the introduction waits its turn.
    pub fn shows_notice(&self) -> bool {
        self.activity.is_live() && matches!(self.activity.kind, Some(ActivityKind::Tool(_) | ActivityKind::Message(_)))
    }
}

#[cfg(test)]
mod tests {
    use notchbase_sdk::protocol::{Node, Surface};

    use super::*;
    use crate::notch::Mode;
    use crate::notch::tests::fixtures::island;

    fn press(slot: ToolSlot, handler: u32) -> Action {
        Action::Tool(slot, Surface::Notice, HandlerId(handler), UiValue::Press)
    }

    fn labels(node: &Node) -> Vec<&str> {
        match node {
            Node::Button { label, .. } => vec![label.as_str()],
            Node::Row { children, .. } | Node::Column { children, .. } => children.iter().flat_map(labels).collect(),
            _ => Vec::new(),
        }
    }

    #[test]
    fn two_buttons_never_one_alone() {
        let notice = intro();
        let controls = notice.controls.as_ref().expect("buttons");
        assert_eq!(labels(controls), [TURN_ON, NOT_NOW, ""], "Turn On first, never a lone button, then the chevron");
        assert!(format!("{:?}", notice.trailing).contains(WHOLE));
        assert!(notice.below.is_none() && !notice.banner, "anything under it would stop an island tucking the buttons");
        assert!(!notice.sticky() && notice.pop);
    }

    #[test]
    fn presses_map_to_answers() {
        assert_eq!(pressed(SLOT, vec![press(SLOT, 0)]), [Action::AnswerDockIntro(true)]);
        assert_eq!(pressed(SLOT, vec![press(SLOT, 1)]), [Action::AnswerDockIntro(false)]);
        assert_eq!(pressed(SLOT, vec![press(SLOT, 2)]), [Action::OpenDockIntro]);
        assert!(pressed(SLOT, Vec::new()).is_empty());
        let elsewhere = press(ToolSlot(1), 0);
        assert_eq!(pressed(ToolSlot(1), vec![elsewhere.clone()]), [elsewhere]);
    }

    #[test]
    fn its_slot_is_nobody_elses() {
        let taken = [super::super::whats_new::SLOT, ToolSlot(u16::MAX), ToolSlot(u16::MAX - 1), ToolSlot(u16::MAX - 2)];
        assert!(!taken.contains(&SLOT));
    }

    #[test]
    fn card_has_the_picture_overview_and_buttons() {
        let card = opened();
        assert!(is_open(&card) && !is_open(&intro()));
        let below = format!("{:?}", card.below);
        assert!(below.contains(crate::tools::DOCK_PICTURE), "the bundled picture");
        for line in OVERVIEW {
            assert!(below.contains(line), "{line}");
        }
        assert_eq!(
            labels(card.controls.as_ref().unwrap()),
            [TURN_ON, NOT_NOW, ""],
            "the same buttons, in the same order"
        );
        assert!(!card.pop, "opening is a change of shape, not a new notice");
        assert_eq!(Duration::from_millis(card.duration_ms), OPEN_FOR);
    }

    #[test]
    fn chevron_opens_and_folds_in_place() {
        let mut n = island();
        n.introduce_dock();
        assert!(n.toggle_dock_intro());
        let shown = |n: &Notch| match &n.activity.kind {
            Some(ActivityKind::Tool(notice)) if notice.slot == SLOT => Some(is_open(&notice.node)),
            _ => None,
        };
        assert_eq!(shown(&n), Some(true));
        assert!(n.activity.is_live());
        assert!(n.toggle_dock_intro());
        assert_eq!(shown(&n), Some(false), "folded back to the compact notice");
        n.close_dock_intro();
        assert!(!n.toggle_dock_intro(), "nothing to open once it has gone");
        let mut other = island();
        other.show_message("Something else");
        assert!(!other.toggle_dock_intro(), "never takes over another notice");
    }

    #[test]
    fn turn_on_lands_on_dock_settings() {
        let mut n = island();
        n.introduce_dock();
        assert!(n.shows_notice());
        n.open_dock_settings();
        assert!(!n.activity.is_live(), "or it would be back when the panel closes");
        assert_eq!((n.mode, n.pager.page), (Mode::Expanded, Page::Settings));
    }
}
