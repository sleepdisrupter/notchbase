use std::time::Instant;

use egui::vec2;

use crate::bridge::{Action, Page};
use crate::notch::notice::whats_new::{SLOT, updated};
use crate::notch::notice::{ISLAND_H, ToolNotice};
use crate::notch::preview::Preview;
use crate::notch::tests::fixtures::{AWAY, REST, Rig, click, island, ms, settle, shortcuts, wake_anywhere_near};
use crate::notch::{Mode, Notch};
use crate::platform::Style;

use super::{CAMERA, DWELL, beside_the_controls, controls, macbook, show};

const VERSION: &str = "0.14.0";
const CLEAR_OF_CAMERA: f32 = 6.0;
const SETTLING_FRAMES: usize = 90;
const CLOSES_AFTER_MS: u64 = 300;

fn posted() -> ToolNotice {
    ToolNotice::new(SLOT, updated(VERSION))
}

fn due() -> Rig {
    let mut rig = Rig::new();
    rig.news = true;
    rig
}

fn frames(rig: &Rig, n: &mut Notch) {
    let keys = shortcuts(None);
    for _ in 0..SETTLING_FRAMES {
        rig.frame(n, &keys, Vec::new());
    }
}

#[test]
fn view_changes_asks_without_opening() {
    for mut n in [island(), macbook()] {
        let rig = due();
        show(&rig, &mut n, posted(), Instant::now());
        let at = controls(&n).center();
        assert!(n.wants_input(at));
        assert_eq!(click(&rig, &mut n, at), [Action::ViewChanges]);
        assert_eq!(n.mode, Mode::Activity);
    }
}

#[test]
fn island_says_update_in_one_capsule() {
    let rig = due();
    let mut n = island();
    show(&rig, &mut n, posted(), Instant::now());
    let (body, at) = (n.silhouette().rect(), controls(&n));
    assert_eq!(body.height(), ISLAND_H);
    assert!(body.contains_rect(at));
    assert!((at.center().y - body.center().y).abs() < 0.01);
    assert!(at.left() > body.center().x, "the words come first, the button ends the line");
    assert!(body.width() <= n.panel_width(), "{} is wider than the panel it opens", body.width());
}

#[test]
fn beside_camera_word_takes_left_wing() {
    for camera in [vec2(185.0, 32.0), vec2(185.0, 38.0), vec2(180.0, 32.0), vec2(200.0, 37.0), vec2(160.0, 30.0)] {
        let rig = due();
        let mut n = Notch::with_preview(Style::Notch, Some(camera), None);
        show(&rig, &mut n, posted(), Instant::now());
        let (body, at) = (n.silhouette().rect(), controls(&n));
        assert!(body.width() <= n.panel_width(), "{camera:?}");
        assert!(at.left() >= body.center().x + camera.x * 0.5 + CLEAR_OF_CAMERA, "{camera:?}: right of the housing");
        assert!(body.contains_rect(at), "{camera:?}: inside the shape");
        assert!(body.height() <= camera.y + 2.0, "{camera:?}: no row underneath");
    }
}

#[test]
fn click_beside_opens_news() {
    for mut n in [island(), macbook()] {
        let rig = due();
        show(&rig, &mut n, posted(), Instant::now());
        let beside = beside_the_controls(&n);
        assert_eq!(click(&rig, &mut n, beside), []);
        assert_eq!((n.mode, n.pager.page), (Mode::Expanded, Page::WhatsNew));
        assert!(!n.activity.is_live(), "or it would be back when the panel closes");
        frames(&rig, &mut n);
        assert_eq!(n.visible_page(), Some(Page::WhatsNew));
    }
}

#[test]
fn seen_notes_open_where_it_was() {
    for mut n in [island(), macbook()] {
        let rig = Rig::new();
        show(&rig, &mut n, posted(), Instant::now());
        let (beside, before) = (beside_the_controls(&n), n.pager.page);
        assert_eq!(click(&rig, &mut n, beside), []);
        assert_eq!((n.mode, n.pager.page), (Mode::Expanded, before));
    }
}

#[test]
fn resting_beside_opens_news() {
    let _config = wake_anywhere_near();
    for mut n in [island(), macbook()] {
        let (rig, t0) = (due(), Instant::now());
        show(&rig, &mut n, posted(), t0);
        let beside = beside_the_controls(&n);
        n.tick(t0, Some(AWAY));
        n.tick(t0, Some(beside));
        n.tick(t0 + REST, Some(beside));
        assert_eq!(n.mode, Mode::Peek);
        n.tick(t0 + REST + DWELL + ms(10), Some(beside));
        assert_eq!((n.mode, n.pager.page), (Mode::Expanded, Page::WhatsNew));
        assert!(!n.activity.is_live());
    }
}

#[test]
fn press_leaves_news_open() {
    for mut n in [island(), macbook()] {
        let (rig, t0) = (due(), Instant::now());
        show(&rig, &mut n, posted(), t0);
        let at = controls(&n).center();
        n.tick(t0 + ms(100), Some(at));
        assert_eq!(click(&rig, &mut n, at), [Action::ViewChanges]);
        n.dismiss_notice(SLOT);
        n.open_page(Page::WhatsNew);
        frames(&rig, &mut n);
        assert_eq!(n.visible_page(), Some(Page::WhatsNew));
        n.tick(t0 + ms(200), Some(at));
        n.tick(t0 + ms(200 + CLOSES_AFTER_MS), Some(at));
        assert_eq!(n.mode, Mode::Expanded, "the pointer that pressed the button is still on the panel");
        n.tick(t0 + ms(1_000), Some(AWAY));
        n.tick(t0 + ms(1_000 + CLOSES_AFTER_MS), Some(AWAY));
        assert_eq!(n.mode, Mode::Idle, "it closes to rest, not back into the notice");
        frames(&rig, &mut n);
        settle(&mut n);
        assert!(n.is_collapsed());
    }
}

#[test]
fn pinned_preview_takes_no_press() {
    for style in [Style::Island, Style::Notch] {
        let rig = Rig::new();
        let mut n = Notch::with_preview(style, Some(CAMERA), Some(Preview::Updated));
        frames(&rig, &mut n);
        let at = controls(&n).center();
        assert_eq!(click(&rig, &mut n, at), []);
        assert_eq!((n.mode, n.visible_page()), (Mode::Activity, None));
    }
}
