//! Where the walkthrough's title, text, buttons and dots go.

use std::sync::Arc;

use egui::text::LayoutJob;
use egui::{Align, Color32, FontId, Galley, Rangef};

use crate::ui::canvas::Canvas;
use crate::ui::font;

pub(super) const COLUMN: f32 = 312.0;
pub(super) const WELL: f32 = 36.0;
pub(super) const MARK: f32 = 16.0;
pub(super) const WELL_GAP: f32 = 10.0;
pub(super) const TITLE_SIZE: f32 = 17.0;
pub(super) const TITLE_GAP: f32 = 3.0;
pub(super) const WHY_ROWS: usize = 2;
pub(super) const HINT_GAP: f32 = 4.0;
pub(super) const BUTTONS_GAP: f32 = 12.0;
pub(super) const BUTTON_H: f32 = 28.0;
pub(super) const BUTTON_MIN: f32 = 112.0;
pub(super) const BUTTON_SPACING: f32 = 10.0;
pub(super) const DOTS_GAP: f32 = 12.0;
pub(super) const DOT: f32 = 5.0;
pub(super) const DOT_SPACING: f32 = 6.0;
pub(super) const CURRENT_DOT: f32 = 14.0;
const EVEN_SLACK: f32 = 16.0;
const LINE: &str = "Ag";

pub(super) fn column(width: f32) -> Rangef {
    let wide = width.clamp(0.0, COLUMN);
    Rangef::new((width - wide) * 0.5, (width + wide) * 0.5)
}

pub(super) fn centered(
    canvas: &Canvas,
    text: &str,
    font: FontId,
    color: Color32,
    width: f32,
    rows: usize,
) -> Arc<Galley> {
    let mut job = LayoutJob::simple(text.to_owned(), font, color, width.max(1.0));
    job.halign = Align::Center;
    job.wrap.max_rows = rows;
    job.wrap.overflow_character = Some('\u{2026}');
    canvas.painter().layout_job(job)
}

pub(super) fn balanced(canvas: &Canvas, text: &str, font: FontId, color: Color32, width: f32) -> Arc<Galley> {
    let natural = canvas.text_width(text, font.clone());
    if natural <= width {
        return centered(canvas, text, font, color, width, WHY_ROWS);
    }
    let even = (natural / WHY_ROWS as f32 + EVEN_SLACK).min(width);
    let galley = centered(canvas, text, font.clone(), color, even, WHY_ROWS);
    if galley.elided { centered(canvas, text, font, color, width, WHY_ROWS) } else { galley }
}

pub(super) fn title_font() -> FontId {
    font::semibold(TITLE_SIZE)
}

pub(super) fn why_font() -> FontId {
    font::callout()
}

pub(super) fn hint_font() -> FontId {
    font::subheadline()
}

pub(super) fn line(canvas: &Canvas, font: FontId) -> f32 {
    canvas.text_size(LINE, font).y.ceil()
}

pub(super) fn title_top() -> f32 {
    WELL + WELL_GAP
}

pub(super) fn why_top(canvas: &Canvas) -> f32 {
    title_top() + line(canvas, title_font()) + TITLE_GAP
}

pub(super) fn hint_top(canvas: &Canvas) -> f32 {
    why_top(canvas) + line(canvas, why_font()) * WHY_ROWS as f32 + HINT_GAP
}

pub(super) fn buttons_top(canvas: &Canvas) -> f32 {
    hint_top(canvas) + line(canvas, hint_font()) + BUTTONS_GAP
}

pub(super) fn dots_top(canvas: &Canvas) -> f32 {
    buttons_top(canvas) + BUTTON_H + DOTS_GAP
}

/// Every card is this tall, so the panel keeps one height from the hello to the last card.
pub(super) fn height(canvas: &Canvas) -> f32 {
    dots_top(canvas) + DOT
}
