//! Single-line text fields. Clicking one takes the keyboard, and Enter, Escape or a click elsewhere hands it back.

pub mod bare;
mod edit;
mod keys;
mod look;

use egui::{Id, Rect, Sense};

use crate::bridge::Action;
use crate::ui::canvas::Canvas;
use crate::ui::controls;

use self::edit::Line;
use self::keys::{Key, type_into};
use self::look::{Look, paint};

const MASK: char = '\u{2022}';
const MOST_MASKED: usize = 24;

pub struct Focus {
    id: Id,
    line: Line,
    rect: Rect,
    seen: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Edit {
    Typed(String),
    Submitted(String),
    Cancelled,
}

pub struct Field<'a> {
    pub id: Id,
    pub rect: Rect,
    pub value: &'a str,
    pub prefix: &'a str,
    pub placeholder: &'a str,
    pub secret: bool,
    pub editable: bool,
}

pub fn field(
    ui: &egui::Ui,
    canvas: &mut Canvas,
    field: &Field,
    focus: &mut Option<Focus>,
    actions: &mut Vec<Action>,
) -> Vec<Edit> {
    let Field { id, rect, value, prefix, placeholder, secret, editable } = *field;
    let response = ui.interact(rect, id, if editable { Sense::click() } else { Sense::hover() });
    let hover = controls::animate(ui, id.with("hover"), editable && response.hovered());
    let focused = focus.as_ref().is_some_and(|focus| focus.id == id);
    if response.clicked() && editable && !focused {
        if focus.is_none() {
            actions.push(Action::Typing(true));
        }
        *focus = Some(Focus { id, line: Line::new(value, false), rect, seen: true });
    }
    let edits =
        focus.as_mut().filter(|current| current.id == id).map_or_else(Vec::new, |current| current.take(ui, rect));
    if edits.iter().any(|edit| matches!(edit, Edit::Submitted(_) | Edit::Cancelled)) {
        release(focus, actions);
    }
    let current = focus.as_ref().filter(|current| current.id == id);
    let look = Look { value, prefix, placeholder, secret };
    paint(canvas, rect, hover, current, &look);
    edits
}

impl Focus {
    pub fn is(&self, id: Id) -> bool {
        self.id == id
    }

    #[cfg(test)]
    pub fn text(&self) -> &str {
        &self.line.text
    }

    fn take(&mut self, ui: &egui::Ui, rect: Rect) -> Vec<Edit> {
        self.seen = true;
        self.rect = rect;
        type_into(ui, &mut self.line).into_iter().map(|key| self.edit(key)).collect()
    }

    fn edit(&self, key: Key) -> Edit {
        match key {
            Key::Typed => Edit::Typed(self.line.text.clone()),
            Key::Submitted => Edit::Submitted(self.line.text.clone()),
            Key::Cancelled => Edit::Cancelled,
        }
    }
}

pub fn begin(focus: &mut Option<Focus>, id: Id, value: &str, rect: Rect, actions: &mut Vec<Action>) {
    if focus.is_none() {
        actions.push(Action::Typing(true));
    }
    *focus = Some(Focus { id, line: Line::new(value, false), rect, seen: true });
}

#[cfg(test)]
pub fn begin_selected(focus: &mut Option<Focus>, id: Id, value: &str, rect: Rect, actions: &mut Vec<Action>) {
    begin(focus, id, value, rect, actions);
    if let Some(focus) = focus.as_mut() {
        focus.line.select_all();
    }
}

/// Lets go after a click outside the field, or when the field wasn't painted this frame.
pub fn end_frame(focus: &mut Option<Focus>, ui: &egui::Ui, actions: &mut Vec<Action>) -> Option<String> {
    let current = focus.as_mut()?;
    let rect = current.rect;
    let pressed_elsewhere =
        ui.input(|i| i.pointer.any_pressed() && i.pointer.press_origin().is_some_and(|at| !rect.contains(at)));
    if current.seen && !pressed_elsewhere {
        current.seen = false;
        return None;
    }
    release(focus, actions)
}

pub fn release(focus: &mut Option<Focus>, actions: &mut Vec<Action>) -> Option<String> {
    let text = let_go(focus)?;
    actions.push(Action::Typing(false));
    Some(text)
}

pub fn let_go(focus: &mut Option<Focus>) -> Option<String> {
    focus.take().map(|left| left.line.text)
}

pub fn masked(text: &str) -> String {
    std::iter::repeat_n(MASK, text.chars().count().min(MOST_MASKED)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(events: Vec<egui::Event>, start: &str) -> String {
        typed_over(events, start, false).0
    }

    fn typed_over(events: Vec<egui::Event>, start: &str, selected: bool) -> (String, bool) {
        let ctx = egui::Context::default();
        let mut line = Line::new(start, selected);
        let mut output = ctx.run_ui(egui::RawInput { events, ..Default::default() }, |ui| {
            type_into(ui, &mut line);
        });
        output.textures_delta.clear();
        (line.text, line.all)
    }

    fn key(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers }
    }

    #[test]
    fn typing_pasting_and_erasing() {
        assert_eq!(typed(vec![egui::Event::Text("hi".into()), egui::Event::Paste("\tthere".into())], ""), "hithere");
        assert_eq!(typed(vec![key(egui::Key::Backspace, egui::Modifiers::NONE)], "abc"), "ab");
        let word = egui::Modifiers { ctrl: true, command: true, ..Default::default() };
        assert_eq!(typed(vec![key(egui::Key::Backspace, word)], "one two three"), "one two ");
    }

    #[test]
    fn typing_over_selection_replaces_it() {
        let text = |text: &str| egui::Event::Text(text.into());
        assert_eq!(typed_over(vec![text("s"), text("am")], "@jane", true), ("sam".into(), false));
        assert_eq!(typed_over(vec![egui::Event::Paste("@ann".into())], "@jane", true), ("@ann".into(), false));
        let erase = key(egui::Key::Backspace, egui::Modifiers::NONE);
        assert_eq!(typed_over(vec![erase.clone()], "@jane", true), (String::new(), false));
        assert_eq!(typed_over(vec![erase], "@jane", false), ("@jan".into(), false));
        let right = key(egui::Key::ArrowRight, egui::Modifiers::NONE);
        assert_eq!(typed_over(vec![right, text("x")], "@jane", true), ("@janex".into(), false));
        let enter = key(egui::Key::Enter, egui::Modifiers::NONE);
        assert_eq!(typed_over(vec![enter], "@jane", true), ("@jane".into(), true));
    }

    #[test]
    fn stock_field_types_at_the_caret() {
        let left = key(egui::Key::ArrowLeft, egui::Modifiers::NONE);
        assert_eq!(typed(vec![left.clone(), left, egui::Event::Text("n".into())], "@jae"), "@jnae");
    }

    #[test]
    fn focus_with_text_selected() {
        let mut focus = None;
        let mut actions = Vec::new();
        begin_selected(&mut focus, Id::new("name"), "jane", Rect::NOTHING, &mut actions);
        assert!(focus.as_ref().is_some_and(|focus| focus.line.all && focus.text() == "jane"));
        begin_selected(&mut focus, Id::new("name"), "", Rect::NOTHING, &mut actions);
        assert!(focus.as_ref().is_some_and(|focus| !focus.line.all));
        assert_eq!(actions, [Action::Typing(true)]);
    }

    #[test]
    fn focus_without_a_click() {
        let mut focus = None;
        let mut actions = Vec::new();
        let id = Id::new("handle");
        begin(&mut focus, id, "@", Rect::NOTHING, &mut actions);
        begin(&mut focus, id, "@j", Rect::NOTHING, &mut actions);
        assert_eq!(actions, [Action::Typing(true)]);
        assert!(focus.as_ref().is_some_and(|focus| focus.is(id) && focus.text() == "@j"));
        assert_eq!(release(&mut focus, &mut actions).as_deref(), Some("@j"));
        assert_eq!(actions, [Action::Typing(true), Action::Typing(false)]);
    }

    #[test]
    fn letting_go_keeps_the_text() {
        let mut focus = None;
        let mut actions = Vec::new();
        begin(&mut focus, Id::new("handle"), "@ja", Rect::NOTHING, &mut actions);
        assert_eq!(let_go(&mut focus).as_deref(), Some("@ja"));
        assert!(focus.is_none());
        assert_eq!(let_go(&mut focus), None);
        assert_eq!(release(&mut focus, &mut actions), None);
        assert_eq!(actions, [Action::Typing(true)]);
    }

    #[test]
    fn secrets_show_as_dots() {
        assert_eq!(masked(""), "");
        assert_eq!(masked("abc"), "\u{2022}\u{2022}\u{2022}");
        assert_eq!(masked(&"x".repeat(200)).chars().count(), MOST_MASKED);
    }
}
