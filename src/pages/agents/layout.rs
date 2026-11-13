//! Row sizes and where the text goes, shared by every row on the Agents page.

use egui::{Align2, Color32, Pos2, Rect, pos2};

use crate::pages::PageCtx;
use crate::pages::settings::{INSET, ROW_H, SECTION_H};
use crate::ui::color::label;
use crate::ui::{controls, font};

pub(super) const LEAD: f32 = 12.0;
pub(super) const TEXT_GAP: f32 = 12.0;
const LINE_OFFSET: f32 = 8.0;

pub(super) fn height(running: usize, recent: usize) -> f32 {
    let live = running as f32 * ROW_H;
    let done = if recent == 0 { 0.0 } else { SECTION_H + recent as f32 * ROW_H };
    if running == 0 && recent == 0 { ROW_H } else { live + done }
}

pub(super) fn lead_center(rect: Rect) -> Pos2 {
    pos2(rect.left() + INSET + LEAD, rect.center().y)
}

pub(super) fn text_left(rect: Rect) -> f32 {
    lead_center(rect).x + LEAD + TEXT_GAP
}

pub(super) fn message_width(rect: Rect) -> f32 {
    (rect.right() - INSET - text_left(rect)).max(0.0)
}

pub(super) fn top_line(rect: Rect, two: bool) -> f32 {
    if two { rect.center().y - LINE_OFFSET } else { rect.center().y }
}

pub(super) fn bottom_line(rect: Rect) -> f32 {
    rect.center().y + LINE_OFFSET
}

pub(super) fn title(ctx: &mut PageCtx<'_>, rect: Rect, text: &str, two: bool, right: f32) {
    let left = text_left(rect);
    let shown = controls::truncate(ctx.canvas, text, font::body(), (right - left).max(0.0));
    ctx.canvas.text(pos2(left, top_line(rect, two)), Align2::LEFT_CENTER, shown, font::body(), label());
}

pub(super) fn detail(ctx: &mut PageCtx<'_>, rect: Rect, text: &str, tone: Color32, right: f32) {
    let left = text_left(rect);
    let shown = controls::truncate(ctx.canvas, text, font::caption(), (right - left).max(0.0));
    ctx.canvas.text(pos2(left, bottom_line(rect)), Align2::LEFT_CENTER, shown, font::caption(), tone);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_row_when_empty() {
        assert_eq!(height(0, 0), ROW_H);
        assert_eq!(height(1, 0), ROW_H);
        assert_eq!(height(2, 0), 2.0 * ROW_H);
        assert_eq!(height(0, 3), SECTION_H + 3.0 * ROW_H);
        assert_eq!(height(2, 1), 2.0 * ROW_H + SECTION_H + ROW_H);
    }

    #[test]
    fn text_starts_past_mark() {
        let rect = Rect::from_min_size(pos2(10.0, 0.0), egui::vec2(340.0, ROW_H));
        let center = lead_center(rect);
        assert_eq!((center.x, center.y), (10.0 + INSET + LEAD, ROW_H * 0.5));
        assert_eq!(text_left(rect), center.x + LEAD + TEXT_GAP);
        assert!(top_line(rect, true) < top_line(rect, false) && top_line(rect, false) < bottom_line(rect));
        assert_eq!(message_width(rect), rect.right() - INSET - text_left(rect));
    }
}
