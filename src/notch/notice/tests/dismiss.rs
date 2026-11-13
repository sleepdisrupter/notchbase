use std::time::Instant;

use egui::{Pos2, pos2};

use crate::notch::geometry::CANVAS;
use crate::notch::notice::controls::CONTROLS_REACH;
use crate::notch::tests::fixtures::{AWAY, REST, Rig, click, hover_open_off, island, ms, wake_anywhere_near};
use crate::notch::{Mode, Notch};

use super::{SLOT, controls, copied, macbook, pressed, show, with_controls};

const POLL_MS: u64 = 20;
const POLLS: u64 = 150;
const CAMERA_ROW: f32 = 16.0;
const WELL_INSIDE: f32 = 10.0;

fn stays_idle_under(n: &mut Notch, from: Instant, at: Pos2) -> Instant {
    for poll in 1..=POLLS {
        n.tick(from + ms(poll * POLL_MS), Some(at));
        assert_eq!(n.mode, Mode::Idle, "{} ms later", poll * POLL_MS);
    }
    assert!(!n.activity.is_live());
    assert_eq!((n.peek_until, n.dwell_until), (None, None));
    from + ms(POLLS * POLL_MS)
}

fn just_above_the_button(n: &Notch) -> Pos2 {
    let pill = controls(n);
    let x = (CANVAS.x * 0.5).clamp(pill.left() + WELL_INSIDE, pill.right() - WELL_INSIDE);
    pos2(x, pill.top() - CONTROLS_REACH + 1.0)
}

fn on_the_camera_row(n: &Notch) -> Pos2 {
    let at = pos2(CANVAS.x * 0.5, n.silhouette().rect().top() + CAMERA_ROW);
    assert!(!n.hovers_controls(Some(at)), "on the notice, away from its button");
    at
}

fn wakes_once_it_has_left_and_come_back(n: &mut Notch, later: Instant, at: Pos2) {
    n.tick(later, Some(AWAY));
    n.tick(later, Some(at));
    n.tick(later + REST, Some(at));
    assert_eq!(n.mode, Mode::Peek);
}

#[test]
fn own_button_dismisses_at_once() {
    for mut n in [island(), macbook()] {
        let (rig, t0) = (Rig::new(), Instant::now());
        show(&rig, &mut n, with_controls(copied()), t0);
        let at = controls(&n).center();
        n.tick(t0 + ms(100), Some(at));
        assert_eq!(click(&rig, &mut n, at), [pressed()]);
        n.dismiss_notice(SLOT);
        stays_idle_under(&mut n, Instant::now(), at);
        assert!(!n.wants_input(at), "a second click goes to what is underneath");
        assert_eq!(click(&rig, &mut n, at), [], "and what is left of the button takes no press");
    }
}

#[test]
fn pointer_wakes_nothing_until_left() {
    let _config = wake_anywhere_near();
    for mut n in [island(), macbook()] {
        let (rig, t0) = (Rig::new(), Instant::now());
        show(&rig, &mut n, with_controls(copied()), t0);
        let at = just_above_the_button(&n);
        n.tick(t0, Some(AWAY));
        n.tick(t0 + ms(100), Some(at));
        assert!(n.hovers_controls(Some(at)));
        n.dismiss_notice(SLOT);
        assert!(n.wake_zone().contains(at), "it is still over the notch at rest");
        let later = stays_idle_under(&mut n, Instant::now(), at);
        wakes_once_it_has_left_and_come_back(&mut n, later, at);
    }
}

#[test]
fn click_beside_ends_sticky() {
    let _config = wake_anywhere_near();
    for mut n in [island(), macbook()] {
        let (rig, t0) = (Rig::new(), Instant::now());
        show(&rig, &mut n, with_controls(copied().sticky()), t0);
        let at = just_above_the_button(&n);
        n.tick(t0, Some(AWAY));
        n.tick(t0 + ms(100), Some(at));
        assert!(n.hovers_controls(Some(at)) && n.wants_input(at));
        assert_eq!(click(&rig, &mut n, at), [], "it misses the button");
        assert!(n.wake_zone().contains(at), "it is still over the notch at rest");
        let later = stays_idle_under(&mut n, Instant::now(), at);
        wakes_once_it_has_left_and_come_back(&mut n, later, at);
    }
}

#[test]
fn click_elsewhere_ends_sticky() {
    let _config = wake_anywhere_near();
    for mut n in [island(), macbook()] {
        let (rig, t0) = (Rig::new(), Instant::now());
        show(&rig, &mut n, with_controls(copied().sticky()), t0);
        let at = on_the_camera_row(&n);
        n.tick(t0, Some(AWAY));
        assert_eq!(click(&rig, &mut n, at), []);
        assert_eq!(n.mode, Mode::Idle);
        let gone = Instant::now();
        n.tick(gone, Some(at));
        n.tick(gone + REST, Some(at));
        assert_eq!(n.mode, Mode::Peek, "as it does after a plain sticky notice");
    }
}

#[test]
fn dismissed_elsewhere_still_peeks() {
    let _config = wake_anywhere_near();
    for mut n in [island(), macbook()] {
        let (rig, t0) = (Rig::new(), Instant::now());
        show(&rig, &mut n, with_controls(copied()), t0);
        let at = on_the_camera_row(&n);
        n.tick(t0, Some(AWAY));
        n.tick(t0, Some(at));
        assert!(n.peek_until.is_some(), "a peek is on its way");
        n.dismiss_notice(SLOT);
        assert_eq!(n.mode, Mode::Idle);
        n.tick(t0 + REST, Some(at));
        assert_eq!(n.mode, Mode::Peek, "as it would after a plain notice");
    }
}

#[test]
fn no_hover_open_still_takes_clicks() {
    let _config = hover_open_off();
    for mut n in [island(), macbook()] {
        let (rig, t0) = (Rig::new(), Instant::now());
        show(&rig, &mut n, with_controls(copied()), t0);
        let at = on_the_camera_row(&n);
        n.tick(t0, Some(AWAY));
        n.tick(t0, Some(at));
        n.dismiss_notice(SLOT);
        assert!(n.wants_input(at), "the click is not let through to what is underneath");
        assert_eq!(click(&rig, &mut n, at), []);
        assert_eq!(n.mode, Mode::Expanded, "and it opens the panel");
    }
}
