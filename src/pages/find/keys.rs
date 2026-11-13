//! Keys on the Find page: arrows, Enter, ⌘Enter and Escape, and telling a real edit from a stray space.

use egui::{Event, Key};

use super::FindPage;
use crate::bridge::{Action, How};
use crate::pages::PageCtx;
use crate::ui::field::Edit;

const STALE_FRAMES: u8 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Nav {
    Up,
    Down,
    Open,
    Reveal,
    Clear,
    Close,
}

pub(super) fn navs(ui: &egui::Ui, edits: &[Edit], empty: bool, holding: bool) -> Vec<Nav> {
    let held = ui.input(|input| input.modifiers.command);
    let mut from_field = from_edits(edits, empty, holding).into_iter();
    let mut navs = Vec::new();
    for event in ui.input(|input| input.events.clone()) {
        if let Some(nav) = arrow(&event) {
            navs.push(nav);
        } else if ends_an_edit(&event) {
            navs.extend(from_field.next().map(|nav| revealing(&event, held, nav)));
        }
    }
    navs
}

fn revealing(event: &Event, held: bool, nav: Nav) -> Nav {
    match event {
        Event::Key { modifiers, .. } if nav == Nav::Open && (held || modifiers.command) => Nav::Reveal,
        _ => nav,
    }
}

fn ends_an_edit(event: &Event) -> bool {
    matches!(event, Event::Key { key: Key::Enter | Key::Escape, pressed: true, .. })
}

fn from_edits(edits: &[Edit], mut empty: bool, mut holding: bool) -> Vec<Nav> {
    let mut navs = Vec::new();
    for edit in edits {
        match edit {
            Edit::Typed(text) => {
                empty = blank(text);
                holding = false;
            }
            Edit::Submitted(_) => navs.push(Nav::Open),
            Edit::Cancelled if holding => {
                navs.push(Nav::Clear);
                holding = false;
            }
            Edit::Cancelled if !empty => {
                navs.push(Nav::Clear);
                empty = true;
            }
            Edit::Cancelled => navs.push(Nav::Close),
        }
    }
    navs
}

fn blank(text: &str) -> bool {
    text.trim().is_empty()
}

fn same_words(one: &str, other: &str) -> bool {
    one.split_whitespace().eq(other.split_whitespace())
}

fn arrow(event: &Event) -> Option<Nav> {
    match event {
        Event::Key { key: Key::ArrowUp, pressed: true, .. } => Some(Nav::Up),
        Event::Key { key: Key::ArrowDown, pressed: true, .. } => Some(Nav::Down),
        _ => None,
    }
}

#[derive(Default)]
pub(super) struct Keyboard {
    confirmed: bool,
    stale: u8,
}

impl Keyboard {
    pub(super) fn lost(&mut self, focused: bool, typing: bool) -> bool {
        if !focused {
            *self = Self::default();
            return false;
        }
        if typing {
            *self = Self { confirmed: true, stale: 0 };
            return false;
        }
        self.stale = self.stale.saturating_add(u8::from(self.confirmed));
        self.stale >= STALE_FRAMES
    }
}

impl FindPage {
    pub(super) fn keys(&mut self, ctx: &mut PageCtx<'_>, edits: &[Edit], actions: &mut Vec<Action>) {
        let (empty, holding) = (self.draft.is_empty(), self.hold.holding());
        for edit in edits {
            if let Edit::Typed(text) = edit {
                self.typed(text, actions);
            }
        }
        for nav in navs(ctx.ui, edits, empty, holding) {
            let count = self.shown.rows.len();
            if self.resting() && matches!(nav, Nav::Down | Nav::Open | Nav::Reveal) {
                self.browsing = true;
                continue;
            }
            match nav {
                Nav::Up | Nav::Down => {
                    self.select.step(if nav == Nav::Up { -1 } else { 1 }, &self.shown.rows);
                    self.list.follow(self.select.index, count, self.list.height);
                }
                Nav::Open | Nav::Reveal => {
                    let how = if nav == Nav::Open { How::Open } else { How::Reveal };
                    match self.shown.answers(&self.draft) {
                        true => actions.extend(self.pick(how, self.select.index, ctx.now)),
                        false => self.hold.hold(how, ctx.now),
                    }
                }
                Nav::Clear if self.hold.holding() => self.hold.drop_it(),
                Nav::Clear => self.clear(actions),
                Nav::Close => actions.push(Action::FindClose),
            }
        }
    }

    fn typed(&mut self, text: &str, actions: &mut Vec<Action>) {
        if blank(text) {
            return self.emptied(actions);
        }
        if !same_words(&self.draft, text) {
            self.select.reset();
        }
        text.clone_into(&mut self.draft);
        self.hold.drop_it();
        actions.push(Action::FindQuery(text.to_owned()));
    }

    fn emptied(&mut self, actions: &mut Vec<Action>) {
        if !self.draft.is_empty() {
            return self.clear(actions);
        }
        if let Some(focus) = self.focus.as_mut() {
            focus.set_text("");
        }
        self.hold.drop_it();
    }

    pub(super) fn clear(&mut self, actions: &mut Vec<Action>) {
        if let Some(focus) = self.focus.as_mut() {
            focus.set_text("");
        }
        self.draft.clear();
        self.browsing = false;
        self.select.reset();
        self.hold.drop_it();
        actions.push(Action::FindQuery(String::new()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_drops_clears_closes() {
        let escape = [Edit::Cancelled];
        assert_eq!(from_edits(&escape, false, true), [Nav::Clear]);
        assert_eq!(from_edits(&[Edit::Cancelled, Edit::Cancelled], false, true), [Nav::Clear, Nav::Clear]);
        assert_eq!(from_edits(&escape, false, false), [Nav::Clear]);
        assert_eq!(from_edits(&escape, true, false), [Nav::Close]);
        assert_eq!(from_edits(&[Edit::Cancelled, Edit::Cancelled], false, false), [Nav::Clear, Nav::Close]);
        let typed_then_escape = [Edit::Typed("c".into()), Edit::Cancelled];
        assert_eq!(from_edits(&typed_then_escape, true, false), [Nav::Clear]);
        let spaces_then_escape = [Edit::Typed("  ".into()), Edit::Cancelled];
        assert_eq!(from_edits(&spaces_then_escape, true, false), [Nav::Close], "spaces leave the field empty");
        assert_eq!(from_edits(&spaces_then_escape, false, false), [Nav::Close], "erased down to spaces, it is empty");
    }

    #[test]
    fn spaces_alone_are_no_text() {
        assert!(blank("") && blank(" ") && blank(" \u{a0}\t ") && !blank(" c") && !blank("/"));
        assert!(same_words("chrome", "chrome ") && same_words(" google  chrome", "google chrome "));
        assert!(!same_words("googlechrome", "google chrome") && !same_words("chrome", "chrom"));
        assert!(!same_words("", "c") && same_words("", "  "));
    }

    #[test]
    fn enter_opens_cmd_enter_reveals() {
        let enter = [Edit::Submitted("car".into())];
        assert_eq!(from_edits(&enter, false, false), [Nav::Open]);
        let press =
            |modifiers| Event::Key { key: Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers };
        assert_eq!(revealing(&press(egui::Modifiers::NONE), false, Nav::Open), Nav::Open);
        assert_eq!(revealing(&press(egui::Modifiers::COMMAND), false, Nav::Open), Nav::Reveal);
        assert_eq!(revealing(&press(egui::Modifiers::NONE), true, Nav::Open), Nav::Reveal);
        assert_eq!(revealing(&press(egui::Modifiers::COMMAND), true, Nav::Close), Nav::Close);
    }

    #[test]
    fn keyboard_lost_after_two_frames() {
        let mut keyboard = Keyboard::default();
        assert!(!keyboard.lost(true, false) && !keyboard.lost(true, false), "never confirmed, never stale");
        assert!(!keyboard.lost(true, true));
        assert!(!keyboard.lost(true, false));
        assert!(keyboard.lost(true, false));
        assert!(!keyboard.lost(false, false));
        assert!(!keyboard.lost(true, false), "a new focus starts over");
    }
}
