use egui::Pos2;

use crate::bridge::{Action, Page};
use crate::notch::Notch;

use super::super::fixtures::{Rig, click, island};
use super::{SETTLING_FRAMES, click_opens, due, frames, macbook, pointer_leaves};

type Way = fn(&Notch) -> Pos2;

const WAYS: [(&str, Way); 3] = [
    ("a tab", |n| n.header().bar.center()),
    ("the gear", |n| n.header().gear.center()),
    ("Find", |n| n.header().find.center()),
];

fn said_seen(actions: &[Action]) -> usize {
    actions.iter().filter(|action| **action == Action::ChangesSeen).count()
}

#[test]
fn walking_away_counts_as_seen() {
    let rig = due();
    for (name, way) in WAYS {
        for mut n in [island(), macbook()] {
            assert_eq!(said_seen(&click_opens(&rig, &mut n)), 0, "{name}: opening onto it says nothing");
            assert_eq!(n.visible_page(), Some(Page::WhatsNew));
            let at = way(&n);
            let walked = click(&rig, &mut n, at);
            assert_ne!(n.pager.page, Page::WhatsNew, "{name}");
            assert_eq!(said_seen(&walked), 1, "{name}: {walked:?}");
            assert_eq!(said_seen(&frames(&rig, &mut n, SETTLING_FRAMES)), 0, "{name}: it is said once");
        }
    }
}

#[test]
fn drifting_off_keeps_news_waiting() {
    let rig = due();
    for mut n in [island(), macbook()] {
        click_opens(&rig, &mut n);
        assert_eq!(pointer_leaves(&rig, &mut n), []);
        assert!(n.is_collapsed());
        click_opens(&rig, &mut n);
        assert_eq!(n.visible_page(), Some(Page::WhatsNew));
    }
}

#[test]
fn seen_notes_say_nothing() {
    for (name, way) in WAYS {
        let (rig, mut n) = (Rig::new(), island());
        n.open_page(Page::WhatsNew);
        frames(&rig, &mut n, SETTLING_FRAMES);
        let at = way(&n);
        let walked = click(&rig, &mut n, at);
        assert_ne!(n.pager.page, Page::WhatsNew, "{name}");
        assert_eq!(said_seen(&walked), 0, "{name}: {walked:?}");
    }
}
