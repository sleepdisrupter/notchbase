//! A text setting being edited. A changed value is saved trimmed, and cancelling drops it.

use egui::Id;
use notchbase_sdk::protocol::Value;

use crate::bridge::{Action, ToolSlot};
use crate::ui::field::{self, Focus};

struct Target {
    id: Id,
    slot: ToolSlot,
    key: String,
    original: String,
}

#[derive(Default)]
pub(super) struct Editing {
    pub(super) focus: Option<Focus>,
    target: Option<Target>,
}

impl Editing {
    pub(super) fn aim(&mut self, id: Id, slot: ToolSlot, key: &str, original: &str) {
        let ours = self.focus.as_ref().is_some_and(|focus| focus.is(id));
        let aimed = self.target.as_ref().is_some_and(|target| target.id == id);
        if ours && !aimed {
            self.target = Some(Target { id, slot, key: key.to_owned(), original: original.to_owned() });
        }
    }

    pub(super) fn submit(&mut self, text: &str) -> Option<Action> {
        let target = self.target.take()?;
        let text = text.trim();
        (text != target.original.trim())
            .then(|| Action::SetToolSetting(target.slot, target.key, Value::String(text.to_owned())))
    }

    pub(super) fn cancel(&mut self) {
        self.target = None;
    }

    pub(super) fn end_frame(&mut self, ui: &egui::Ui, actions: &mut Vec<Action>) {
        if let Some(text) = field::end_frame(&mut self.focus, ui, actions) {
            actions.extend(self.submit(&text));
        }
    }

    pub(super) fn let_go(&mut self) -> Option<Action> {
        let committed = field::let_go(&mut self.focus).and_then(|text| self.submit(&text));
        self.target = None;
        committed
    }

    pub(super) fn finish(&mut self, actions: &mut Vec<Action>) {
        if let Some(text) = field::release(&mut self.focus, actions) {
            actions.extend(self.submit(&text));
        }
        self.target = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn aimed(original: &str) -> Editing {
        let mut editing = Editing::default();
        let id = Id::new("field");
        editing.target = Some(Target { id, slot: ToolSlot(2), key: "link".into(), original: original.into() });
        editing
    }

    #[test]
    fn saves_only_changed_trimmed() {
        let saved = aimed("old").submit("  https://example.com/cal.ics \t");
        assert_eq!(
            saved,
            Some(Action::SetToolSetting(
                ToolSlot(2),
                "link".into(),
                Value::String("https://example.com/cal.ics".into())
            ))
        );
        assert_eq!(aimed("same").submit(" same "), None);
    }

    #[test]
    fn letting_go_keeps_typed() {
        let mut editing = aimed("old");
        let mut actions = Vec::new();
        field::begin(&mut editing.focus, Id::new("field"), "new", egui::Rect::NOTHING, &mut actions);
        let saved = Action::SetToolSetting(ToolSlot(2), "link".into(), Value::String("new".into()));
        assert_eq!(editing.let_go(), Some(saved));
        assert!(editing.focus.is_none() && editing.target.is_none());
        assert_eq!(editing.let_go(), None);
        assert_eq!(actions, [Action::Typing(true)], "no Typing(false)");
    }

    #[test]
    fn cancelling_forgets_the_field() {
        let mut editing = aimed("old");
        editing.cancel();
        assert_eq!(editing.submit("new"), None);
    }
}
