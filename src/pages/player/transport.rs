//! The previous, play/pause and next buttons.

use egui::{Id, Pos2, Rect, Sense, Vec2, pos2, vec2};

use crate::media::Control;
use crate::pages::PageCtx;
use crate::ui::color::{label, white};
use crate::ui::controls;
use crate::ui::glyph::{self, Glyph};

const SPACING: f32 = 62.0;
const BUTTON: Vec2 = vec2(48.0, 36.0);
const MAIN_GLYPH: f32 = 23.0;
const SIDE_GLYPH: f32 = 19.0;
const PRESSED: f32 = 0.88;

pub(super) fn ui(ctx: &mut PageCtx<'_>, center: Pos2, playing: bool, interactive: bool) -> Option<Control> {
    let main = if playing { Glyph::Pause } else { Glyph::Play };
    let buttons = [
        (Control::Previous, Glyph::Backward, -SPACING, SIDE_GLYPH),
        (Control::PlayPause, main, 0.0, MAIN_GLYPH),
        (Control::Next, Glyph::Forward, SPACING, SIDE_GLYPH),
    ];
    let mut chosen = None;
    for (i, (control, glyph, offset, size)) in buttons.into_iter().enumerate() {
        let rect = Rect::from_center_size(pos2(center.x + offset, center.y), BUTTON);
        let id = Id::new(("notchbase.media.transport", i));
        let sense = if interactive { Sense::click() } else { Sense::hover() };
        let response = ctx.ui.interact(rect, id, sense);
        let hover = controls::animate(ctx.ui, id.with("hover"), interactive && response.hovered());
        let pressed = controls::animate(ctx.ui, id.with("press"), interactive && response.is_pointer_button_down_on());
        if hover > 0.001 {
            let halo = Rect::from_center_size(rect.center(), Vec2::splat(BUTTON.y));
            ctx.canvas.rect(halo, BUTTON.y * 0.5, white(0.1 * hover));
        }
        let scale = 1.0 - (1.0 - PRESSED) * pressed;
        glyph::paint(ctx.canvas, glyph, rect.center(), size * scale, label());
        if response.clicked() && interactive {
            chosen = Some(control);
        }
    }
    chosen
}
