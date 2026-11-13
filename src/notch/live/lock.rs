//! The lock screen: the padlock in the notch, and a new window taking over once it unlocks.

use std::time::{Duration, Instant};

use egui::{Pos2, Rect, Shape, Stroke, Vec2, pos2, vec2};

use crate::bridge::Activity;
use crate::notch::activity::LANE_INSET;
use crate::notch::geometry::Geometry;
use crate::notch::moves::transition::Motion;
use crate::notch::{Mode, Notch};
use crate::platform::Style;
use crate::ui::canvas::Canvas;
use crate::ui::color::{self, label};
use crate::ui::spring::{Spring, lerp, smoothstep};

pub(in crate::notch) const GLYPH: f32 = 15.0;
const OPEN_DELAY: Duration = Duration::from_millis(120);
const OPEN_TIME: Duration = Duration::from_millis(460);
const SHOWN_OPEN: Duration = Duration::from_millis(1400);
const PUT_AWAY: Duration = Duration::from_millis(1200);
const SHRUNK: f32 = 0.6;
const OVERSHOOT: f32 = 1.70158;
const ARC_STEPS: usize = 18;
const REACH: f32 = 0.2;
const BODY_TOP: f32 = -0.08;
const SHACKLE_REST: f32 = -0.12;
const LIFT: f32 = 0.17;
const LOOSE_LEG: f32 = 0.05;

#[derive(Clone, Copy, Debug)]
pub(in crate::notch) struct Leaving {
    since: Instant,
    wings: Option<Geometry>,
}

impl Notch {
    pub fn set_locked(&mut self, locked: bool) {
        if self.preview.is_some() || self.locked == locked {
            return;
        }
        self.locked = locked;
        self.lock_leaving = None;
        if locked {
            self.unlocked_at = None;
            self.pinned = false;
            self.follow_card(Motion::Dismiss);
            if matches!(self.mode, Mode::Expanded | Mode::Peek | Mode::Drop) {
                self.collapse();
            }
        } else {
            self.unlocked_at = Some(Instant::now());
            self.hover_armed = false;
            self.follow_card(Motion::Pop);
        }
        self.combine_activities();
    }

    pub fn above_lock_screen(&self) -> bool {
        self.locked || self.unlocked_at.is_some() || self.lock_leaving.is_some()
    }

    pub fn take_over(&mut self, spent: &Notch) {
        if self.preview.is_some() {
            return;
        }
        self.hover_armed = spent.hover_armed;
        self.music_until = spent.music_until;
        self.show_ask(spent.ask.clone());
        self.arrange();
        self.settle();
        self.taking_over = true;
    }

    pub fn settle_in(&mut self) -> bool {
        let measured = std::mem::take(&mut self.taking_over);
        if measured {
            self.settle();
        }
        measured
    }

    fn settle(&mut self) {
        self.retarget(Motion::Resize);
        self.snap();
        self.settle_sides();
    }

    pub(super) fn lock_activity(&self) -> Option<Activity> {
        if self.locked {
            return Some(Activity::Lock(false));
        }
        self.unlocked_at.map(|_| Activity::Lock(true))
    }

    pub(in crate::notch) fn lock_tick(&mut self, now: Instant) -> bool {
        let shown = self.unlocked_at.is_some_and(|at| now.saturating_duration_since(at) >= SHOWN_OPEN);
        if shown {
            let leads = self.live.as_ref().is_some_and(|live| matches!(live.activity, Activity::Lock(_)));
            self.unlocked_at = None;
            self.lock_leaving = Some(Leaving { since: now, wings: leads.then(|| self.live_geometry()) });
            self.combine_activities();
        }
        let resting = matches!(self.mode, Mode::Idle | Mode::Ask) && !self.activity.is_live();
        let fades = [&self.attract, &self.live_fade, &self.ask_fade];
        let still = self.morph.springs().into_iter().chain(fades).all(Spring::is_settled);
        let tucked = !self.sides.iter().any(|side| matches!(side.activity, Activity::Lock(_)));
        let Some(leaving) = &mut self.lock_leaving else { return shown };
        if !resting {
            leaving.since = now;
            return shown;
        }
        let put_away = (still && tucked) || now.saturating_duration_since(leaving.since) >= PUT_AWAY;
        if put_away {
            self.lock_leaving = None;
        }
        shown || put_away
    }

    pub(in crate::notch) fn lock_wakeup(&self) -> Option<Instant> {
        let leaving = self.lock_leaving.map(|leaving| leaving.since + PUT_AWAY);
        self.unlocked_at.map(|at| at + SHOWN_OPEN).or(leaving)
    }

    pub(in crate::notch) fn lock_opening(&self) -> bool {
        self.unlocked_at.is_some_and(|at| at.elapsed() < OPEN_DELAY + OPEN_TIME)
    }

    pub(super) fn lock_share(&self) -> f32 {
        let left_open = if self.lock_leaving.is_some() { 1.0 } else { 0.0 };
        self.unlocked_at.map_or(left_open, |at| opened(at.elapsed()))
    }

    pub(in crate::notch) fn lock_leaving_ui(&self, ui: &egui::Ui, g: Geometry, body: Rect) {
        let Some(wings) = self.lock_leaving.and_then(|leaving| leaving.wings) else { return };
        let shown = self.lock_left(g, wings);
        if shown <= 0.001 {
            return;
        }
        let center = match self.style {
            Style::Notch => pos2(self.lead(body.left() + LANE_INSET) + GLYPH * 0.5, body.top() + self.notch.y * 0.5),
            Style::Island => body.center(),
        };
        let mut canvas = Canvas::new(ui.painter());
        paint_padlock(&mut canvas, center, GLYPH * lerp(SHRUNK, 1.0, shown), 1.0);
        let clip = ui.painter().with_clip_rect(body.expand(1.0));
        canvas.paint(&clip, pos2(body.center().x, body.top()), 1.0, Vec2::ZERO, shown);
    }

    fn lock_left(&self, g: Geometry, wings: Geometry) -> f32 {
        let idle = self.idle_geometry();
        let room = match self.style {
            Style::Notch => smoothstep(0.15, 0.8, (g.w - idle.w) / (wings.w - idle.w).max(1.0)),
            Style::Island => smoothstep(0.35, 1.0, self.attract.value),
        };
        let replaced = if self.live.is_some() { self.live_fade.value } else { 0.0 };
        let fades = [self.music_fade, self.activity_fade, self.ask_fade, self.panel_fade, self.drop_fade];
        let covered = fades.iter().map(|fade| fade.value).fold(replaced, f32::max);
        room * (1.0 - covered.clamp(0.0, 1.0))
    }
}

pub(in crate::notch) fn opened(elapsed: Duration) -> f32 {
    let t = (elapsed.saturating_sub(OPEN_DELAY).as_secs_f32() / OPEN_TIME.as_secs_f32()).clamp(0.0, 1.0);
    let back = t - 1.0;
    1.0 + (OVERSHOOT + 1.0) * back.powi(3) + OVERSHOOT * back.powi(2)
}

pub(super) fn paint_padlock(canvas: &mut Canvas, center: Pos2, size: f32, open: f32) {
    let tint = color::mix(label(), color::GREEN, open.clamp(0.0, 1.0));
    let p = |x: f32, y: f32| center + vec2(x, y) * size;
    let stroke = Stroke::new((size * 0.11).max(1.3), tint);
    canvas.rect(Rect::from_min_max(p(-0.32, -0.1), p(0.32, 0.42)), 0.1 * size, tint);
    let top = SHACKLE_REST - LIFT * open;
    let loose = BODY_TOP + (top + LOOSE_LEG - BODY_TOP) * open.clamp(0.0, 1.0);
    let pi = std::f32::consts::PI;
    let mut bow = vec![p(-REACH, loose)];
    bow.extend((0..=ARC_STEPS).map(|step| {
        let angle = pi + pi * step as f32 / ARC_STEPS as f32;
        p(REACH * angle.cos(), top + REACH * angle.sin())
    }));
    bow.push(p(REACH, BODY_TOP));
    canvas.add(Shape::line(bow, stroke));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::{Ask, Choices};
    use crate::notch::tests::fixtures::{OVER, island, ms, notch, settle};

    fn prompt() -> Ask {
        Ask {
            id: 7,
            provider: crate::assistant::Provider::Claude,
            context: "notchbase".into(),
            title: "Run cargo test?".into(),
            detail: None,
            choices: Choices::Permission { always: false },
            waiting: 0,
        }
    }

    #[test]
    fn shackle_springs_open() {
        assert_eq!(opened(Duration::ZERO), 0.0);
        assert_eq!(opened(OPEN_DELAY), 0.0);
        let peak = (1..20).map(|step| opened(OPEN_DELAY + OPEN_TIME * step / 20)).fold(0.0, f32::max);
        assert!(peak > 1.0, "it overshoots a little");
        assert!((opened(OPEN_DELAY + OPEN_TIME) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn locked_shows_padlock() {
        let mut n = island();
        n.set_locked(true);
        assert_eq!(n.activities.first(), Some(&Activity::Lock(false)));
        let now = Instant::now();
        n.tick(now, Some(OVER));
        n.tick(now + Duration::from_secs(1), Some(OVER));
        settle(&mut n);
        assert!(!n.wants_input(OVER), "nothing on the lock screen can be clicked");
        assert!(matches!(n.mode, Mode::Idle | Mode::Activity), "it never peeks or opens");
        n.set_pinned(true);
        n.open_page(crate::bridge::Page::Shelf);
        assert!(n.visible_page().is_none(), "a tool or the app can't open the panel either");
    }

    #[test]
    fn lock_puts_panel_away() {
        let mut n = island();
        n.show_ask(Some(prompt()));
        settle(&mut n);
        assert_eq!(n.mode, Mode::Ask);
        n.set_locked(true);
        assert_ne!(n.mode, Mode::Ask, "a Claude prompt never sits on the lock screen");
        n.show_ask(Some(Ask { id: 8, ..prompt() }));
        assert_ne!(n.mode, Mode::Ask);
        n.set_locked(false);
        assert_eq!(n.mode, Mode::Ask, "it comes back once you're in");
    }

    #[test]
    fn unlock_opens_padlock() {
        let mut n = island();
        n.set_locked(true);
        settle(&mut n);
        assert!(n.above_lock_screen());
        n.set_locked(false);
        assert!(n.above_lock_screen(), "it stays above the fading lock screen while the padlock opens");
        assert_eq!(n.activities.first(), Some(&Activity::Lock(true)));
        assert!(n.lock_opening());
        assert!(n.lock_wakeup().is_some());
        let left = Instant::now() + SHOWN_OPEN + ms(10);
        n.tick(left, None);
        assert!(n.activities.is_empty());
        assert!(n.above_lock_screen(), "it stays up while the island tucks the padlock away");
        assert_eq!(n.lock_share(), 1.0, "still open as it goes");
        settle(&mut n);
        n.tick(left + ms(20), None);
        assert!(n.lock_wakeup().is_none());
        assert!(!n.above_lock_screen(), "then it steps down so files can be dropped on it again");
    }

    #[test]
    fn padlock_folds_in_first() {
        let mut n = notch();
        n.set_locked(true);
        settle(&mut n);
        n.set_locked(false);
        let wings = n.morph.current();
        assert!(wings.w > n.idle_geometry().w);
        let left = Instant::now() + SHOWN_OPEN;
        n.tick(left, None);
        assert_eq!(
            n.lock_left(n.morph.current(), wings),
            1.0,
            "the open padlock is in full view as the wing starts to fold"
        );
        let (mut shown, mut frames) = (1.0, 0);
        while n.above_lock_screen() {
            frames += 1;
            assert!(frames < 120, "it steps down within two seconds");
            n.step(1.0 / 60.0);
            let now = n.lock_left(n.morph.current(), wings);
            assert!(now <= shown + 1e-4, "the padlock only ever fades out");
            shown = now;
            n.tick(left + ms(frames * 1000 / 60), None);
        }
        assert_eq!(shown, 0.0, "it has faded out before the window steps down");
        assert!((n.morph.current().w - n.idle_geometry().w).abs() < 0.1, "and the wing is back in the notch");
        assert!(frames > 15, "the fold is seen, not cut short");
    }

    #[test]
    fn panel_after_unlock_waits() {
        let mut n = notch();
        n.set_locked(true);
        n.set_locked(false);
        n.open_page(crate::bridge::Page::Shelf);
        settle(&mut n);
        let left = Instant::now() + SHOWN_OPEN;
        n.tick(left, Some(OVER));
        settle(&mut n);
        let later = left + PUT_AWAY * 2;
        n.tick(later, Some(OVER));
        assert!(n.above_lock_screen(), "a new window would come up closed under the pointer");
        n.collapse();
        n.tick(later, None);
        assert!(n.above_lock_screen(), "the panel folds away first");
        settle(&mut n);
        n.tick(later + ms(20), None);
        assert!(!n.above_lock_screen());
    }

    #[test]
    fn new_window_takes_over() {
        let live = crate::notch::live::tests::recording(notchbase_sdk::now_ms());
        let mut spent = notch();
        spent.set_activities(vec![live.clone()]);
        spent.hover_armed = false;
        settle(&mut spent);
        let mut fresh = notch();
        fresh.set_activities(vec![live]);
        fresh.take_over(&spent);
        assert_eq!(fresh.morph.current(), spent.morph.current());
        assert!(!fresh.is_animating(), "nothing pops in a second time");
        assert!(fresh.live_alpha(fresh.morph.current()) > 0.99);
        assert!(!fresh.hover_armed, "and it doesn't peek at a pointer the old one was ignoring");
        assert!(fresh.settle_in(), "what its first frame measured settles before it is shown");
        assert!(!fresh.settle_in());
        assert!(!notch().settle_in(), "a window that took nothing over opens as it always did");
    }

    #[test]
    fn waiting_prompt_returns() {
        let mut spent = notch();
        spent.show_ask(Some(prompt()));
        spent.set_locked(true);
        settle(&mut spent);
        spent.set_locked(false);
        assert_eq!(spent.mode, Mode::Ask);
        let left = Instant::now() + SHOWN_OPEN;
        spent.tick(left, None);
        settle(&mut spent);
        spent.tick(left + ms(20), None);
        assert!(!spent.above_lock_screen(), "a card waiting for an answer doesn't hold the window up");
        let mut fresh = notch();
        fresh.take_over(&spent);
        assert_eq!(fresh.mode, Mode::Ask);
        assert_eq!(fresh.morph.current(), spent.morph.current());
        assert_eq!(fresh.ask_fade.value, 1.0, "the card is already in place");
    }
}
