//! Paints a run's message under its row, a line at a time as the row unfolds.

use egui::{Align2, Color32, Rect, pos2};

use super::layout::{bottom_line, detail, text_left};
use super::wrap::Wrapped;
use crate::pages::PageCtx;
use crate::ui::font;
use crate::ui::spring::smoothstep;

const HANDOFF: f32 = 0.5;
const LEAD_IN: f32 = 0.6;

pub(super) struct Message<'a> {
    pub(super) first: &'a str,
    pub(super) wrapped: &'a Wrapped,
    pub(super) extra: f32,
}

pub(super) fn paint(ctx: &mut PageCtx<'_>, head: Rect, message: &Message<'_>, tone: Color32, right: f32) {
    let line_h = message.wrapped.line_h;
    let open = if line_h > 0.0 { smoothstep(0.0, line_h * HANDOFF, message.extra) } else { 0.0 };
    if open < 1.0 {
        detail(ctx, head, message.first, tone.gamma_multiply(1.0 - open), right);
    }
    if open <= 0.0 {
        return;
    }
    let (left, first_y) = (text_left(head), bottom_line(head));
    for (index, line) in message.wrapped.lines.iter().enumerate() {
        let shown = if index == 0 { open } else { reveal(index, message.extra, line_h) };
        if shown > 0.0 {
            let at = pos2(left, first_y + index as f32 * line_h);
            ctx.canvas.text(at, Align2::LEFT_CENTER, line.as_str(), font::caption(), tone.gamma_multiply(shown));
        }
    }
}

fn reveal(index: usize, extra: f32, line_h: f32) -> f32 {
    let full = index as f32 * line_h;
    smoothstep(full - LEAD_IN * line_h, full, extra)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_fade_in_with_room() {
        let line_h = 12.0;
        assert_eq!(reveal(1, 0.0, line_h), 0.0);
        assert_eq!(reveal(1, line_h, line_h), 1.0);
        assert_eq!(reveal(2, line_h, line_h), 0.0);
        let half = reveal(2, 1.7 * line_h, line_h);
        assert!(half > 0.0 && half < 1.0);
        assert_eq!(reveal(2, 2.0 * line_h, line_h), 1.0);
    }
}
