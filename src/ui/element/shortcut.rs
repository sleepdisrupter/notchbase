//! Shows a tool's shortcut as keycaps, or Add Shortcut when it has none. Clicking it records a new one.

use egui::emath::TSTransform;
use egui::{Align2, CornerRadius, Id, Pos2, Rect, Sense, Shadow, Vec2, pos2, vec2};

use super::measure::KEYCAP_FONT;
use super::{Measure, Scene};
use crate::bridge::Action;
use crate::tools::shortcuts::{self, ShortcutView};
use crate::ui::canvas::Canvas;
use crate::ui::color::{self, label, secondary_label, white};
use crate::ui::glass::{self, Glass};
use crate::ui::glyph::{self, Glyph};
use crate::ui::{controls, font};

pub const PROMPT: &str = "Type shortcut…";
pub const KEYCAP_H: f32 = 17.0;
pub const KEYS_SLOP: f32 = 4.0;
const ADD: &str = "Add Shortcut";
const PROMPT_SHIFT: f32 = 2.0;
const PROMPT_PAD: f32 = 22.0;
const PROMPT_H: f32 = 20.0;
const PROMPT_RADIUS: u8 = 10;
const CLEAR: f32 = 18.0;
const CLEAR_GAP: f32 = 6.0;
const CLEAR_GLYPH: f32 = 9.0;
const ADD_PAD: f32 = 16.0;
const ADD_RADIUS: f32 = 5.0;
const KEYS_HIGHLIGHT: f32 = 3.0;
const MISSING: &str = "Unknown shortcut";

pub fn size(measure: &Measure, id: &str) -> Vec2 {
    let Some(view) = shortcuts::find(measure.env.shortcuts, measure.slot, id) else {
        return measure.canvas.text_size(MISSING, font::caption());
    };
    if view.recording {
        let text = message(measure.env.prompt, view);
        let clear = if view.keys.is_empty() { 0.0 } else { CLEAR + CLEAR_GAP };
        vec2(measure.canvas.text_width(text, font::subheadline()) + PROMPT_PAD + clear, PROMPT_H)
    } else if view.keys.is_empty() {
        vec2(measure.canvas.text_width(ADD, font::semibold(KEYCAP_FONT)) + ADD_PAD, KEYCAP_H)
    } else {
        keycaps_size(measure.canvas, &view.keys)
    }
}

pub fn keycaps_size(canvas: &Canvas, keys: &[String]) -> Vec2 {
    let mut probe = Canvas::new(canvas.painter());
    controls::keycaps(&mut probe, Pos2::ZERO, keys, 1.0).size()
}

pub fn paint(scene: &mut Scene, id: &str, rect: Rect) {
    let Some(view) = shortcuts::find(scene.env.shortcuts, scene.slot, id).cloned() else {
        let at = rect.left_center();
        scene.canvas.text(at, Align2::LEFT_CENTER, MISSING, font::caption(), secondary_label());
        return;
    };
    let base = Id::new(("notchbase.keys", scene.slot.0, id));
    let (area, clear) = if view.recording {
        paint_prompt(scene, &view, rect)
    } else if view.keys.is_empty() {
        (paint_add(scene.canvas, rect), None)
    } else {
        (place_keycaps(scene.canvas, rect, &view.keys), None)
    };
    scene.keys.push(area);
    if !scene.live {
        return;
    }
    let keys = scene.ui.interact(area.expand(KEYS_SLOP), base, Sense::click());
    if !view.recording {
        let hover = controls::animate(scene.ui, base.with("hover"), keys.hovered());
        controls::row_highlight(scene.canvas, area.expand(KEYS_HIGHLIGHT), hover);
    }
    let cleared = clear.is_some_and(|button| {
        scene.ui.interact(button.expand(KEYS_SLOP * 0.5), base.with("clear"), Sense::click()).clicked()
    });
    if cleared {
        scene.actions.push(Action::ClearShortcut(view.which));
    } else if keys.clicked() {
        scene.actions.push(if view.recording { Action::CancelRecording } else { Action::RecordShortcut(view.which) });
    }
}

fn message(prompt: super::Prompt, view: &ShortcutView) -> &str {
    let hint = prompt.hint.filter(|(which, _)| *which == view.which).map(|(_, hint)| hint);
    view.problem.as_deref().or(hint).unwrap_or(PROMPT)
}

fn paint_prompt(scene: &mut Scene, view: &ShortcutView, rect: Rect) -> (Rect, Option<Rect>) {
    let prompt = scene.env.prompt;
    let text = message(prompt, view);
    let problem = text != PROMPT;
    let clearable = !view.keys.is_empty();
    let room = rect.width() + PROMPT_SHIFT - if clearable { CLEAR + CLEAR_GAP } else { 0.0 };
    let shown = controls::truncate(scene.canvas, text, font::subheadline(), room - PROMPT_PAD);
    let width = (scene.canvas.text_width(shown.as_str(), font::subheadline()) + PROMPT_PAD).min(room);
    let left = rect.left() - PROMPT_SHIFT + prompt.shake;
    let field = Rect::from_min_size(pos2(left, rect.center().y - PROMPT_H * 0.5), vec2(width, PROMPT_H));
    let tint = if problem { color::ORANGE } else { color::ACCENT };
    let glow = Shadow { offset: [0, 0], blur: 14, spread: 1, color: tint.gamma_multiply(0.25 + 0.3 * prompt.pulse) };
    scene.canvas.add(glow.as_shape(field, CornerRadius::same(PROMPT_RADIUS)));
    glass::paint(scene.canvas, field, f32::from(PROMPT_RADIUS), Glass::tinted_by(tint));
    scene.canvas.text(field.center(), Align2::CENTER_CENTER, shown, font::subheadline(), label());
    let clear = clearable.then(|| {
        let center = pos2(field.right() + CLEAR_GAP + CLEAR * 0.5, field.center().y);
        Rect::from_center_size(center, Vec2::splat(CLEAR))
    });
    if let Some(button) = clear {
        glass::paint(scene.canvas, button, CLEAR * 0.5, Glass::CONTROL);
        glyph::paint(scene.canvas, Glyph::Xmark, button.center(), CLEAR_GLYPH, secondary_label());
    }
    (clear.map_or(field, |button| field.union(button)), clear)
}

pub fn place_keycaps(canvas: &mut Canvas, rect: Rect, keys: &[String]) -> Rect {
    let mut caps = Canvas::new(canvas.painter());
    let bounds = controls::keycaps(&mut caps, Pos2::ZERO, keys, 1.0);
    let fit = (rect.width() / bounds.width().max(1.0)).min(1.0);
    let at = pos2(rect.left(), rect.center().y);
    let place = TSTransform::from_translation(at.to_vec2()) * TSTransform::from_scaling(fit);
    for mut shape in caps.shapes {
        shape.transform(place);
        canvas.add(shape);
    }
    Rect::from_min_size(at + bounds.min.to_vec2() * fit, bounds.size() * fit)
}

fn paint_add(canvas: &mut Canvas, rect: Rect) -> Rect {
    let font = font::semibold(KEYCAP_FONT);
    let width = canvas.text_width(ADD, font.clone()) + ADD_PAD;
    let add = Rect::from_min_size(pos2(rect.left(), rect.center().y - KEYCAP_H * 0.5), vec2(width, KEYCAP_H));
    canvas.rect_stroke(add, ADD_RADIUS, egui::Stroke::new(1.0, white(0.18)));
    canvas.text(add.center(), Align2::CENTER_CENTER, ADD, font, white(0.5));
    add
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::ToolSlot;
    use crate::tools::shortcuts::ShortcutRef;
    use crate::ui::element::Prompt;

    fn view(problem: Option<&str>) -> ShortcutView {
        ShortcutView {
            which: ShortcutRef { tool: ToolSlot(1), index: 0 },
            id: "capture".into(),
            title: "Capture".into(),
            keys: Vec::new(),
            recording: true,
            problem: problem.map(str::to_owned),
        }
    }

    #[test]
    fn recording_asks_for_keys() {
        let quiet = Prompt::default();
        assert_eq!(message(quiet, &view(None)), PROMPT);
        assert_eq!(message(quiet, &view(Some("Taken by Find"))), "Taken by Find");
    }

    #[test]
    fn hint_shows_only_for_its_shortcut() {
        let ours = ShortcutRef { tool: ToolSlot(1), index: 0 };
        let theirs = ShortcutRef { tool: ToolSlot(2), index: 0 };
        let hint = |which| Prompt { hint: Some((which, "Add a key")), ..Prompt::default() };
        assert_eq!(message(hint(ours), &view(None)), "Add a key");
        assert_eq!(message(hint(theirs), &view(None)), PROMPT);
        assert_eq!(message(hint(ours), &view(Some("Taken"))), "Taken", "a problem outranks a hint");
    }

    #[test]
    fn keycaps_shrink_but_never_grow() {
        let ctx = egui::Context::default();
        crate::ui::font::install(&ctx);
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let keys = ["Ctrl".to_owned(), "Shift".to_owned(), "S".to_owned()];
            let mut canvas = Canvas::new(ui.painter());
            let natural = keycaps_size(&canvas, &keys);
            let wide = Rect::from_min_size(pos2(10.0, 0.0), vec2(natural.x * 3.0, 40.0));
            let placed = place_keycaps(&mut canvas, wide, &keys);
            assert!((placed.width() - natural.x).abs() < 1e-3 && placed.left() == 10.0);
            let narrow = Rect::from_min_size(pos2(10.0, 0.0), vec2(natural.x * 0.5, 40.0));
            let squeezed = place_keycaps(&mut canvas, narrow, &keys);
            assert!((squeezed.width() - narrow.width()).abs() < 1e-3, "{squeezed:?}");
            assert!((squeezed.center().y - narrow.center().y).abs() < 1e-3);
            assert!(!canvas.shapes.is_empty());
        });
        output.textures_delta.clear();
    }
}
