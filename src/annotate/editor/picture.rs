//! The screenshot as a texture, patched only where a change touched it, or a note when it's too large to show.

use egui::epaint::{RectShape, Shadow};
use egui::{Align2, Color32, CornerRadius, Rect, TextureFilter, TextureHandle, TextureOptions, pos2, vec2};

use crate::annotate::{PixelRect, Sheet};
use crate::ui::canvas::Canvas;
use crate::ui::{color, font};

const NAME: &str = "annotate";
const SHADOW: Shadow = Shadow { offset: [0, 10], blur: 30, spread: 0, color: Color32::from_black_alpha(120) };
pub(super) const WHOLE: Rect = Rect { min: pos2(0.0, 0.0), max: pos2(1.0, 1.0) };
const TOO_LARGE: &str = "This screenshot is too large to draw on";
const LINE_GAP: f32 = 12.0;

pub(super) enum Picture {
    Waiting,
    Shown(TextureHandle),
    TooLarge(usize),
}

impl Picture {
    pub(super) fn load(&mut self, ctx: &egui::Context, sheet: &Sheet) {
        if !matches!(self, Self::Waiting) {
            return;
        }
        let limit = ctx.input(|input| input.max_texture_side);
        *self = if sheet.size().into_iter().any(|side| side > limit) {
            Self::TooLarge(limit)
        } else {
            Self::Shown(ctx.load_texture(NAME, sheet.image(), options()))
        };
    }

    pub(super) fn is_drawable(&self) -> bool {
        matches!(self, Self::Shown(_))
    }

    pub(super) fn refresh(&mut self, sheet: &Sheet, dirty: PixelRect) {
        let Self::Shown(texture) = self else { return };
        if let Some(patch) = sheet.patch(dirty) {
            texture.set_partial(dirty.origin(), patch, options());
        }
    }

    pub(super) fn paint(&self, canvas: &mut Canvas, rect: Rect, stage: Rect) {
        match self {
            Self::Waiting => {}
            Self::Shown(texture) => {
                canvas.add(SHADOW.as_shape(rect, CornerRadius::ZERO));
                canvas
                    .add(RectShape::filled(rect, CornerRadius::ZERO, Color32::WHITE).with_texture(texture.id(), WHOLE));
            }
            Self::TooLarge(limit) => {
                let (at, lift) = (stage.center(), vec2(0.0, LINE_GAP));
                canvas.text(at - lift, Align2::CENTER_CENTER, TOO_LARGE, font::body(), color::label());
                let limit = format!("This display can show {limit} pixels a side");
                canvas.text(at + lift, Align2::CENTER_CENTER, limit, font::callout(), color::secondary_label());
            }
        }
    }
}

pub(super) fn options() -> TextureOptions {
    TextureOptions::LINEAR.with_mipmap_mode(Some(TextureFilter::Linear))
}
