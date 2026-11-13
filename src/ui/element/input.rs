//! The text input node, built on the shared text field.

use egui::{Id, Rect};
use notchbase_sdk::protocol::{HandlerId, UiValue};

use super::{Scene, SurfaceState};
use crate::bridge::Action;
use crate::ui::field::{self, Edit, Field};

pub(super) fn paint(
    scene: &mut Scene,
    rect: Rect,
    id: Id,
    value: &str,
    placeholder: &str,
    on_change: Option<HandlerId>,
    on_submit: Option<HandlerId>,
) {
    let editable = scene.live && (on_change.is_some() || on_submit.is_some());
    let input = Field { id, rect, value, prefix: "", placeholder, secret: false, editable };
    let edits = field::field(scene.ui, scene.canvas, &input, &mut scene.state.focus, scene.actions);
    for edit in edits {
        match edit {
            Edit::Typed(text) => scene.emit(on_change, UiValue::Text(text)),
            Edit::Submitted(_) => scene.emit(on_submit, UiValue::Press),
            Edit::Cancelled => {}
        }
    }
}

pub(super) fn end_frame(state: &mut SurfaceState, ui: &egui::Ui, actions: &mut Vec<Action>) {
    field::end_frame(&mut state.focus, ui, actions);
}
