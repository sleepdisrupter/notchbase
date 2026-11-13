//! What the Agents page shows with nothing running.

use egui::{Align2, Rect, pos2};

use super::layout::top_line;
use super::words;
use crate::assistant::Provider;
use crate::pages::PageCtx;
use crate::pages::settings::INSET;
use crate::ui::color::{secondary_label, tertiary_label};
use crate::ui::{controls, font};

const BELOW: f32 = 16.0;

pub(super) fn paint(ctx: &mut PageCtx<'_>, rect: Rect, assistant: Option<Provider>) {
    let room = (rect.width() - 2.0 * INSET).max(0.0);
    let x = rect.center().x;
    let top = top_line(rect, true);
    let headline = controls::truncate(ctx.canvas, words::nothing(), font::body(), room);
    ctx.canvas.text(pos2(x, top), Align2::CENTER_CENTER, headline, font::body(), secondary_label());
    let hint = controls::truncate(ctx.canvas, words::who_shows_up(assistant), font::caption(), room);
    ctx.canvas.text(pos2(x, top + BELOW), Align2::CENTER_CENTER, hint, font::caption(), tertiary_label());
}
