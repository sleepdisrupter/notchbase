mod dismiss;
mod reach;
mod shape;
mod updated;

use std::time::{Duration, Instant};

use egui::{Pos2, Rect, pos2, vec2};
use notchbase_sdk::Notice;
use notchbase_sdk::element::lower_static;
use notchbase_sdk::prelude::{button, text};
use notchbase_sdk::protocol::{HandlerId, NoticeNode, Surface, UiValue};

use crate::bridge::{Action, ToolSlot};
use crate::notch::activity::ActivityKind;
use crate::notch::moves::surge::SURGE;
use crate::notch::tests::fixtures::{
    AWAY, OVER, REST, Rig, click, hover_open_off, island, ms, peek, sample_notice, settle, shortcuts,
    wake_anywhere_near,
};
use crate::notch::{Mode, Notch};
use crate::platform::Style;

use super::ToolNotice;

const SLOT: ToolSlot = ToolSlot(1);
const CAMERA: egui::Vec2 = vec2(185.0, 32.0);
const SHOWN_MS: u64 = 2_000;
const DWELL: Duration = Duration::from_millis(120 + SURGE.as_millis() as u64);

fn with_controls(shown: Notice) -> ToolNotice {
    let annotate = lower_static(button("Annotate").on_press(()));
    ToolNotice::new(SLOT, NoticeNode { controls: Some(annotate), ..shown.into_node() })
}

fn copied() -> Notice {
    Notice::new(text("Copied"), text("2560×1600")).duration(ms(SHOWN_MS))
}

fn macbook() -> Notch {
    Notch::with_preview(Style::Notch, Some(CAMERA), None)
}

fn show(rig: &Rig, n: &mut Notch, notice: ToolNotice, now: Instant) {
    n.start_activity(ActivityKind::Tool(notice), now);
    let keys = shortcuts(None);
    for _ in 0..90 {
        rig.frame(n, &keys, Vec::new());
    }
    settle(n);
}

fn showing(n: &Notch) -> &ToolNotice {
    let Some(ActivityKind::Tool(notice)) = &n.activity.kind else { panic!("a tool notice") };
    notice
}

fn controls(n: &Notch) -> Rect {
    n.controls_rect(n.silhouette().rect(), showing(n))
}

fn beside_the_controls(n: &Notch) -> Pos2 {
    let body = n.silhouette().rect();
    pos2(body.left() + 20.0, body.top() + 16.0)
}

fn pressed() -> Action {
    Action::Tool(SLOT, Surface::Notice, HandlerId(0), UiValue::Press)
}

#[test]
fn control_press_tells_its_tool() {
    for mut n in [island(), macbook()] {
        let rig = Rig::new();
        show(&rig, &mut n, with_controls(copied()), Instant::now());
        let at = controls(&n).center();
        assert!(n.wants_input(at));
        assert_eq!(click(&rig, &mut n, at), [pressed()]);
        assert_eq!(n.mode, Mode::Activity);
    }
}

#[test]
fn resting_on_controls_stays_shut() {
    let _config = wake_anywhere_near();
    for mut n in [island(), macbook()] {
        let (rig, t0) = (Rig::new(), Instant::now());
        show(&rig, &mut n, with_controls(copied()), t0);
        let at = controls(&n).center();
        assert!(n.wake_zone().contains(at), "the whole shape wakes it");
        n.tick(t0, Some(AWAY));
        for step in 0..40 {
            n.tick(t0 + ms(step * 100), Some(at));
            assert_eq!(n.mode, Mode::Activity);
        }
        assert_eq!((n.peek_until, n.dwell_until), (None, None));
    }
}

#[test]
fn hovered_control_keeps_notice() {
    for mut n in [island(), macbook()] {
        let (rig, t0) = (Rig::new(), Instant::now());
        show(&rig, &mut n, with_controls(copied()), t0);
        let at = controls(&n).center();
        n.tick(t0 + ms(1_900), Some(at));
        assert_eq!(n.next_wakeup(), Some(t0 + ms(1_900) + super::controls::HELD_FOR));
        n.tick(t0 + ms(2_900), Some(at));
        n.tick(t0 + ms(6_000), Some(at));
        assert_eq!(n.mode, Mode::Activity, "long past its time it is still there");
        n.tick(t0 + ms(6_100), Some(AWAY));
        assert_eq!(n.next_wakeup(), Some(t0 + ms(6_000) + super::controls::HELD_FOR));
        assert!(n.tick(t0 + ms(7_050), Some(AWAY)));
        assert_eq!(n.mode, Mode::Idle);
    }
}

#[test]
fn pointer_elsewhere_keeps_nothing() {
    for mut n in [island(), macbook()] {
        let (rig, t0) = (Rig::new(), Instant::now());
        show(&rig, &mut n, with_controls(copied()), t0);
        let beside = beside_the_controls(&n);
        n.tick(t0 + ms(1_900), Some(beside));
        assert_eq!(n.activity.until, Some(t0 + ms(SHOWN_MS)));
    }
}

#[test]
fn hover_elsewhere_peeks_and_opens() {
    let _config = wake_anywhere_near();
    for mut n in [island(), macbook()] {
        let (rig, t0) = (Rig::new(), Instant::now());
        show(&rig, &mut n, with_controls(copied()), t0);
        let beside = beside_the_controls(&n);
        n.tick(t0, Some(AWAY));
        n.tick(t0, Some(beside));
        n.tick(t0 + REST, Some(beside));
        assert_eq!(n.mode, Mode::Peek);
        n.tick(t0 + REST + DWELL + ms(10), Some(beside));
        assert_eq!(n.mode, Mode::Expanded);
    }
}

#[test]
fn click_elsewhere_opens_panel() {
    for mut n in [island(), macbook()] {
        let rig = Rig::new();
        show(&rig, &mut n, with_controls(copied()), Instant::now());
        let beside = beside_the_controls(&n);
        assert_eq!(click(&rig, &mut n, beside), []);
        assert_eq!(n.mode, Mode::Expanded);
    }
}

#[test]
fn peek_to_controls_drops_back() {
    let _config = wake_anywhere_near();
    for mut n in [island(), macbook()] {
        let (rig, t0) = (Rig::new(), Instant::now());
        show(&rig, &mut n, with_controls(copied()), t0);
        let beside = beside_the_controls(&n);
        n.tick(t0, Some(AWAY));
        n.tick(t0, Some(beside));
        n.tick(t0 + REST, Some(beside));
        assert_eq!(n.mode, Mode::Peek);
        let at = controls(&n).center();
        assert!(n.tick(t0 + REST + ms(50), Some(at)));
        assert_eq!(n.mode, Mode::Activity);
        n.tick(t0 + REST + DWELL + ms(50), Some(at));
        assert_eq!(n.mode, Mode::Activity);
    }
}

#[test]
fn locked_notch_ignores_controls() {
    for mut n in [island(), macbook()] {
        let (rig, t0) = (Rig::new(), Instant::now());
        show(&rig, &mut n, with_controls(copied()), t0);
        let at = controls(&n).center();
        n.set_locked(true);
        assert!(!n.wants_input(at));
        assert_eq!(click(&rig, &mut n, at), []);
        n.tick(t0 + ms(1_900), Some(at));
        assert_eq!(n.activity.until, Some(t0 + ms(SHOWN_MS)), "and the pointer does not keep the notice");
    }
}

#[test]
fn no_hover_open_control_takes_press() {
    let _config = hover_open_off();
    for mut n in [island(), macbook()] {
        let rig = Rig::new();
        show(&rig, &mut n, with_controls(copied()), Instant::now());
        let at = controls(&n).center();
        assert!(n.wants_input(at));
        assert_eq!(click(&rig, &mut n, at), [pressed()]);
        assert_eq!(n.mode, Mode::Activity);
    }
}

#[test]
fn controls_during_peek_stay_clear() {
    let mut n = island();
    let t0 = peek(&mut n, Instant::now(), OVER);
    assert_eq!(n.mode, Mode::Peek);
    n.start_activity(ActivityKind::Tool(with_controls(copied())), t0);
    assert_eq!(n.mode, Mode::Activity);
    for step in 0..19 {
        n.tick(t0 + ms(step * 100), Some(OVER));
        assert_eq!(n.mode, Mode::Activity, "the pointer that was already there opens nothing");
    }
    n.tick(t0 + ms(SHOWN_MS + 10), Some(OVER));
    assert_eq!(n.mode, Mode::Idle);
    n.tick(t0 + ms(SHOWN_MS + 20), Some(AWAY));
    peek(&mut n, t0 + ms(SHOWN_MS + 30), OVER);
    assert_eq!(n.mode, Mode::Peek, "once it has left and come back it wakes as usual");
}

#[test]
fn plain_notice_during_peek_opens() {
    let mut n = island();
    let t0 = peek(&mut n, Instant::now(), OVER);
    n.start_activity(ActivityKind::Tool(sample_notice(SHOWN_MS)), t0);
    assert_eq!(n.mode, Mode::Peek);
    n.tick(t0 + DWELL + ms(10), Some(OVER));
    assert_eq!(n.mode, Mode::Expanded);
}
