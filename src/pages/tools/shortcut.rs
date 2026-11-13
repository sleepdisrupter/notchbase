//! The shortcut keycaps shown on an icon.

use egui::{Align2, Rect};

use crate::platform::hotkey_caption;
use crate::tools::shortcuts::{self, ShortcutView};
use crate::ui::canvas::{Canvas, fade};
use crate::ui::color::tertiary_label;
use crate::ui::element::{Scene, shortcut as field};
use crate::ui::{controls, font};

pub(super) fn paint(scene: &mut Scene, id: &str, rect: Rect, hover: f32) {
    let env = scene.env;
    let view = shortcuts::find(env.shortcuts, scene.slot, id);
    let shown = reveal(view, hover);
    if let Some(view) = view.filter(|_| shown < 1.0) {
        paint_caption(scene.canvas, view, rect, 1.0 - shown);
    }
    let mark = scene.canvas.mark();
    field::paint(scene, id, rect);
    for shape in scene.canvas.since(mark) {
        fade(shape, shown);
    }
}

fn reveal(view: Option<&ShortcutView>, hover: f32) -> f32 {
    match view {
        Some(view) if !view.recording => hover.clamp(0.0, 1.0),
        _ => 1.0,
    }
}

fn paint_caption(canvas: &mut Canvas, view: &ShortcutView, rect: Rect, alpha: f32) {
    if view.keys.is_empty() {
        return;
    }
    let text = controls::truncate(canvas, &hotkey_caption(&view.keys), font::caption(), rect.width());
    canvas.text(rect.left_center(), Align2::LEFT_CENTER, text, font::caption(), tertiary_label().gamma_multiply(alpha));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::ToolSlot;
    use crate::tools::shortcuts::ShortcutRef;

    fn view(recording: bool) -> ShortcutView {
        ShortcutView {
            which: ShortcutRef { tool: ToolSlot(1), index: 0 },
            id: "copy".into(),
            title: "Copy".into(),
            keys: vec!["Ctrl".into(), "C".into()],
            recording,
            problem: None,
        }
    }

    #[test]
    fn keycaps_follow_hover() {
        assert_eq!(reveal(Some(&view(false)), 0.0), 0.0);
        assert_eq!(reveal(Some(&view(false)), 0.4), 0.4);
        assert_eq!(reveal(Some(&view(false)), 1.3), 1.0);
    }

    #[test]
    fn recording_shows_prompt() {
        assert_eq!(reveal(Some(&view(true)), 0.0), 1.0);
    }

    #[test]
    fn unknown_shortcut_always_shows() {
        assert_eq!(reveal(None, 0.0), 1.0);
    }
}
