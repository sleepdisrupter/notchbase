//! Find and the keyboard: it's open on one display at a time and closes when the focus goes elsewhere.

use std::time::{Duration, Instant};

use winit::window::WindowId;

use crate::app::App;
use crate::bridge::Action;
use crate::platform;

const KEPT_AT_MOST: u8 = 4;
const SWITCHES_AFTER: f32 = 1.0;
const FOCUS_CHECKED_EVERY: Duration = Duration::from_millis(250);

#[derive(Debug, PartialEq, Eq)]
struct Handover {
    leave: Vec<usize>,
    keyboard: Keyboard,
}

#[derive(Debug, PartialEq, Eq)]
enum Keyboard {
    Untouched,
    Moves { from: usize, to: usize },
}

#[derive(Debug, PartialEq, Eq)]
enum FocusLoss {
    Ignored,
    Released,
    Kept,
    Collapsed,
}

fn handover(displays: usize, active: usize, opener: usize, typing: bool) -> Handover {
    let leave = (0..displays).filter(|display| *display != opener).collect();
    let keyboard =
        if typing && active != opener { Keyboard::Moves { from: active, to: opener } } else { Keyboard::Untouched };
    Handover { leave, keyboard }
}

#[derive(Clone, Copy, Debug)]
struct Lost {
    active: bool,
    finding: bool,
    holding: bool,
    dragging: bool,
    left: bool,
    kept: u8,
}

#[derive(Clone, Copy, Debug)]
struct Signs {
    launching: bool,
    pointer_outside: bool,
    pressed_since_opening: bool,
    switching: bool,
}

fn left(signs: Signs) -> bool {
    signs.launching || signs.switching || (signs.pointer_outside && signs.pressed_since_opening)
}

fn focus_loss(lost: Lost) -> FocusLoss {
    match lost {
        Lost { active: false, .. } => FocusLoss::Ignored,
        Lost { finding: false, .. } | Lost { dragging: true, .. } => FocusLoss::Released,
        Lost { left: true, .. } => FocusLoss::Collapsed,
        Lost { holding: false, .. } => FocusLoss::Released,
        Lost { kept, .. } if kept >= KEPT_AT_MOST => FocusLoss::Collapsed,
        Lost { .. } => FocusLoss::Kept,
    }
}

fn pressed_away(finding: bool, pointer_outside: bool, dragging: bool) -> bool {
    finding && pointer_outside && !dragging
}

impl App {
    pub(in crate::app) fn find_shown(&mut self, shown: bool) {
        match self.notches.active().map(|nw| nw.window().id()) {
            Some(id) if shown => self.find_opened_on(id),
            _ if shown => self.find.shown(platform::front_app_name()),
            _ => self.find_left(),
        }
    }

    pub(in crate::app) fn find_opened_on(&mut self, id: WindowId) {
        let ids: Vec<WindowId> = self.notches.iter().map(|nw| nw.window().id()).collect();
        let Some(opener) = ids.iter().position(|window| *window == id) else { return };
        let active = (0..ids.len()).find(|index| self.notches.is_active(*index)).unwrap_or(opener);
        let Handover { leave, keyboard } = handover(ids.len(), active, opener, self.typing);
        if let Keyboard::Moves { from, .. } = keyboard {
            self.let_go_quietly(ids[from]);
        }
        self.notches.focus(id);
        let mut committed = Vec::new();
        for (index, nw) in self.notches.windows_mut() {
            if leave.contains(&index) {
                committed.extend(nw.notch.leave_find());
                nw.redraw();
            }
        }
        for action in committed {
            self.commit_quietly(action);
        }
        if matches!(keyboard, Keyboard::Moves { .. }) {
            self.hold_keyboard();
        }
        self.find.shown(platform::front_app_name());
    }

    fn commit_quietly(&mut self, action: Action) {
        if let Action::SetToolSetting(slot, key, value) = action {
            self.set_tool_setting(slot, key, value);
        }
    }

    fn let_go_quietly(&mut self, id: WindowId) {
        let held = !self.held_by.is_empty();
        if let Some(nw) = self.notches.window_mut(id) {
            nw.notch.set_pinned(held);
            platform::end_keyboard_focus(nw.window());
            nw.redraw();
        }
    }

    pub(in crate::app) fn keep_find_focused(&mut self, now: Instant) {
        if !self.typing || self.find.lifting.is_some() || self.find.kept >= KEPT_AT_MOST {
            return;
        }
        if self.find.focus_checked.is_some_and(|at| now.saturating_duration_since(at) < FOCUS_CHECKED_EVERY) {
            return;
        }
        self.find.focus_checked = Some(now);
        let unfocused = self.notches.active().filter(|nw| nw.notch.finding() && !nw.window().has_focus());
        if let Some(id) = unfocused.map(|nw| nw.window().id()) {
            self.notch_lost_focus(id);
        }
    }

    pub(in crate::app) fn notch_lost_focus(&mut self, id: WindowId) {
        let index = self.notches.iter().position(|nw| nw.window().id() == id);
        let active = index.is_some_and(|index| self.notches.is_active(index));
        let window = self.notches.iter().find(|nw| nw.window().id() == id);
        let finding = window.is_some_and(|nw| nw.notch.finding());
        let open_for = window.and_then(|nw| nw.notch.find_open_for());
        let pointer_outside = window.is_none_or(|nw| nw.cursor().is_none_or(|at| !nw.notch.wants_input(at)));
        let pressed = |since: f32| platform::mouse_held() || platform::pressed_within(f64::from(since));
        let held = platform::held_modifiers();
        let signs = Signs {
            launching: self.find.lifting.is_some(),
            pointer_outside,
            pressed_since_opening: open_for.is_some_and(pressed),
            switching: open_for.is_some_and(|since| since > SWITCHES_AFTER) && (held.command || held.alt || held.ctrl),
        };
        let lost = Lost {
            active,
            finding,
            holding: self.typing || self.keys.is_some(),
            dragging: self.drops.dragging_out(),
            left: left(signs),
            kept: self.find.kept,
        };
        match focus_loss(lost) {
            FocusLoss::Ignored => {}
            FocusLoss::Released => self.recording_lost_focus(),
            FocusLoss::Kept => {
                self.find.kept = self.find.kept.saturating_add(1);
                self.hold_keyboard();
            }
            FocusLoss::Collapsed => self.went_elsewhere(id),
        }
    }

    pub(in crate::app) fn close_find_on_a_press_elsewhere(&mut self) {
        if !self.find.pressed_elsewhere() {
            return;
        }
        let Some(nw) = self.notches.active() else { return };
        let pointer_outside = nw.cursor().is_none_or(|at| !nw.notch.wants_input(at));
        if pressed_away(nw.notch.finding(), pointer_outside, self.drops.dragging_out()) {
            let id = nw.window().id();
            self.went_elsewhere(id);
        }
    }

    fn went_elsewhere(&mut self, id: WindowId) {
        self.recording_lost_focus();
        if let Some(nw) = self.notches.window_mut(id) {
            nw.notch.collapse();
            nw.redraw();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_display_at_a_time() {
        let moved = handover(2, 0, 1, true);
        assert_eq!(moved, Handover { leave: vec![0], keyboard: Keyboard::Moves { from: 0, to: 1 } });
        assert_eq!(handover(3, 2, 2, true), Handover { leave: vec![0, 1], keyboard: Keyboard::Untouched });
        assert_eq!(handover(2, 0, 1, false).keyboard, Keyboard::Untouched, "nothing to move when nobody types");
    }

    const HELD: Lost = Lost { active: true, finding: true, holding: true, dragging: false, left: false, kept: 0 };
    const NOTHING: Signs =
        Signs { launching: false, pointer_outside: true, pressed_since_opening: false, switching: false };

    #[test]
    fn inactive_window_focus_ignored() {
        for finding in [true, false] {
            assert_eq!(focus_loss(Lost { active: false, finding, ..HELD }), FocusLoss::Ignored);
        }
        assert_eq!(focus_loss(Lost { finding: false, ..HELD }), FocusLoss::Released);
    }

    #[test]
    fn slipped_focus_taken_back() {
        assert_eq!(focus_loss(HELD), FocusLoss::Kept, "nobody went anywhere: the system handed it back");
        assert_eq!(focus_loss(Lost { kept: KEPT_AT_MOST - 1, ..HELD }), FocusLoss::Kept);
        assert_eq!(focus_loss(Lost { kept: KEPT_AT_MOST, ..HELD }), FocusLoss::Collapsed, "it is not fought over");
        assert_eq!(focus_loss(Lost { dragging: true, ..HELD }), FocusLoss::Released, "a drag is on its way out");
    }

    #[test]
    fn going_elsewhere_closes_find() {
        assert_eq!(focus_loss(Lost { left: true, ..HELD }), FocusLoss::Collapsed);
        assert_eq!(focus_loss(Lost { left: true, holding: false, ..HELD }), FocusLoss::Collapsed);
        assert!(!left(NOTHING));
        assert!(left(Signs { pressed_since_opening: true, ..NOTHING }), "a click or a tap away");
        let on_the_notch = Signs { pointer_outside: false, pressed_since_opening: true, ..NOTHING };
        assert!(!left(on_the_notch), "the click that opened Find, or a second one on its button");
        assert!(left(Signs { switching: true, ..NOTHING }), "another app was switched to by key");
        assert!(left(Signs { launching: true, pointer_outside: false, ..NOTHING }), "what Find launched took over");
    }

    #[test]
    fn press_off_notch_closes_find() {
        assert!(pressed_away(true, true, false), "the desktop, a window, the menu bar or the Dock");
        assert!(!pressed_away(true, false, false), "the notch itself, reached a moment before its clicks were");
        assert!(!pressed_away(true, true, true), "a result being dragged out");
        assert!(!pressed_away(false, true, false), "nothing to close on any other page");
    }

    #[test]
    fn returned_keyboard_stays_returned() {
        let lost = focus_loss(Lost { holding: false, ..HELD });
        assert_eq!(lost, FocusLoss::Released, "nothing is held, so nothing is pinned open and nothing closes");
    }
}
