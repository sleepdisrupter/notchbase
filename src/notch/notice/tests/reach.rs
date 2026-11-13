use std::time::Instant;

use egui::{Pos2, Rect, pos2};
use notchbase_sdk::prelude::text;

use crate::notch::notice::ToolNotice;
use crate::notch::notice::controls::{CONTROLS_REACH, HELD_FOR};
use crate::notch::tests::fixtures::{AWAY, REST, Rig, click, island, ms, wake_anywhere_near};
use crate::notch::{Mode, Notch};

use super::{DWELL, SHOWN_MS, controls, copied, macbook, show, with_controls};

const FROM_THE_EDGE: f32 = 20.0;
const NEARLY_OVER_MS: u64 = 1_900;
const STEP_MS: u64 = 10;

fn under_the_text() -> ToolNotice {
    with_controls(copied().below(text("On the clipboard")))
}

fn hung_underneath() -> [(Notch, ToolNotice); 2] {
    [(island(), under_the_text()), (macbook(), with_controls(copied()))]
}

fn beside_the_button(n: &Notch) -> Pos2 {
    let at = pos2(n.silhouette().rect().left() + FROM_THE_EDGE, controls(n).center().y);
    assert!(n.silhouette().rect().contains(at) && n.wants_input(at), "on the shape, level with the button");
    at
}

fn edges(of: Rect) -> [Pos2; 4] {
    [of.left_center(), of.right_center(), of.center_top(), of.center_bottom()]
}

#[test]
fn only_controls_keep_notice() {
    for (mut n, notice) in
        [(island(), with_controls(copied())), (island(), under_the_text()), (macbook(), with_controls(copied()))]
    {
        let (rig, t0) = (Rig::new(), Instant::now());
        show(&rig, &mut n, notice, t0);
        let (pill, mut now) = (controls(&n), t0 + ms(NEARLY_OVER_MS));
        for at in edges(pill.expand(CONTROLS_REACH + 1.0)) {
            n.tick(now, Some(at));
            assert_eq!(n.activity.until, Some(t0 + ms(SHOWN_MS)), "{at:?} is past the margin");
        }
        for at in edges(pill.expand(CONTROLS_REACH - 1.0)) {
            now += ms(STEP_MS);
            n.tick(now, Some(at));
            assert_eq!(n.activity.until, Some(now + HELD_FOR), "{at:?} is within it");
        }
    }
}

#[test]
fn beside_hung_button_keeps_nothing() {
    for (mut n, notice) in hung_underneath() {
        let (rig, t0) = (Rig::new(), Instant::now());
        show(&rig, &mut n, notice, t0);
        let beside = beside_the_button(&n);
        n.tick(t0 + ms(NEARLY_OVER_MS), Some(beside));
        assert_eq!(n.activity.until, Some(t0 + ms(SHOWN_MS)));
        assert!(n.tick(t0 + ms(SHOWN_MS + STEP_MS), Some(beside)));
        assert_eq!(n.mode, Mode::Idle, "it leaves on time");
    }
}

#[test]
fn beside_hung_button_hover_opens() {
    let _config = wake_anywhere_near();
    for (mut n, notice) in hung_underneath() {
        let (rig, t0) = (Rig::new(), Instant::now());
        show(&rig, &mut n, notice, t0);
        let beside = beside_the_button(&n);
        n.tick(t0, Some(AWAY));
        n.tick(t0, Some(beside));
        n.tick(t0 + REST, Some(beside));
        assert_eq!(n.mode, Mode::Peek);
        n.tick(t0 + REST + DWELL + ms(STEP_MS), Some(beside));
        assert_eq!(n.mode, Mode::Expanded);
    }
}

#[test]
fn beside_hung_button_click_opens() {
    for (mut n, notice) in hung_underneath() {
        let rig = Rig::new();
        show(&rig, &mut n, notice, Instant::now());
        let beside = beside_the_button(&n);
        assert_eq!(click(&rig, &mut n, beside), []);
        assert_eq!(n.mode, Mode::Expanded);
    }
}
