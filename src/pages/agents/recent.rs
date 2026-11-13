//! Paints one finished run.

use egui::{Align2, Color32, Rect, pos2, vec2};

use super::layout::{LEAD, TEXT_GAP, lead_center, title, top_line};
use super::message::{self, Message};
use super::words;
use crate::assistant::Finished;
use crate::pages::PageCtx;
use crate::pages::settings::{INSET, ROW_H};
use crate::ui::color::{self, secondary_label, tertiary_label};
use crate::ui::font;
use crate::ui::glyph::{self, Glyph};

const CHECK: f32 = 11.0;

pub(super) fn paint(ctx: &mut PageCtx<'_>, rect: Rect, done: &Finished, now_ms: u64, message: &Message<'_>) {
    let head = Rect::from_min_size(rect.min, vec2(rect.width(), ROW_H));
    let badge = lead_center(head);
    ctx.canvas.circle(badge, LEAD, color::GREEN);
    glyph::paint(ctx.canvas, Glyph::Checkmark, badge, CHECK, Color32::WHITE);
    let two = !message.first.is_empty();
    let right = head.right() - INSET;
    let meta = words::finished(done, now_ms);
    let width = ctx.canvas.text_width(meta.as_str(), font::caption());
    let at = pos2(right, top_line(head, two));
    ctx.canvas.text(at, Align2::RIGHT_CENTER, meta, font::caption(), tertiary_label());
    title(ctx, head, &done.project, two, right - width - TEXT_GAP);
    if two {
        message::paint(ctx, head, message, secondary_label(), right);
    }
}
