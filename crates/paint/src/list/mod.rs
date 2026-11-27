//! Settings-style cards: rows on glass with hairlines between them, two-line labels and switch rows.

use std::ops::Range;

use egui::{Align2, Color32, FontId, Id, Pos2, Rect, Vec2, pos2, vec2};

use crate::canvas::Canvas;
use crate::color::{label, secondary_label, white};
use crate::glass::{self, Glass};
use crate::metrics::CARD_RADIUS;
use crate::switch::switch;
use crate::{controls, font};

pub const INSET: f32 = 14.0;
pub const TEXT_GAP: f32 = 12.0;
pub const HAIRLINE: f32 = 0.08;
pub const SWITCH: Vec2 = vec2(40.0, 24.0);
const LINE_OFFSET: f32 = 8.0;
const HAIRLINE_WIDTH: f32 = 1.0;
const LIT_PROBE: &str = "x";
const LIT_PAD: f32 = 2.0;
const LIT_RADIUS: f32 = 4.0;
const LIT_ALPHA: f32 = 0.17;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Cut {
    #[default]
    End,
    Start,
}

/// A run of one line lit behind, as a search marks what it matched. Byte offsets into the title or the detail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lit {
    Title(usize, usize),
    Detail(usize, usize),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lines<'a> {
    pub title: &'a str,
    pub detail: &'a str,
    pub tone: Color32,
    pub cut: Cut,
    pub lit: Option<Lit>,
}

impl<'a> Lines<'a> {
    pub fn new(title: &'a str, detail: &'a str) -> Self {
        Self { title, detail, tone: secondary_label(), cut: Cut::End, lit: None }
    }

    pub fn tone(self, tone: Color32) -> Self {
        Self { tone, ..self }
    }

    pub fn cut(self, cut: Cut) -> Self {
        Self { cut, ..self }
    }

    pub fn lit(self, lit: Option<Lit>) -> Self {
        Self { lit, ..self }
    }
}

pub fn card(canvas: &mut Canvas, rect: Rect, heights: &[f32]) -> Vec<Rect> {
    glass::paint(canvas, rect, CARD_RADIUS, Glass::TILE);
    let rows = stacked(rect, heights);
    for row in rows.iter().take(heights.len().saturating_sub(1)) {
        divider(canvas, *row, INSET);
    }
    rows
}

pub fn stacked(card: Rect, heights: &[f32]) -> Vec<Rect> {
    let mut top = card.top();
    heights
        .iter()
        .map(|height| {
            let row = Rect::from_min_size(pos2(card.left(), top), vec2(card.width(), *height));
            top += height;
            row
        })
        .collect()
}

pub fn divider(canvas: &mut Canvas, row: Rect, inset: f32) {
    let line = Rect::from_min_size(
        pos2(row.left() + inset, row.bottom() - HAIRLINE_WIDTH * 0.5),
        vec2(row.width() - inset, HAIRLINE_WIDTH),
    );
    hairline(canvas, line);
}

pub fn hairline(canvas: &mut Canvas, line: Rect) {
    canvas.rect(line, 0.0, white(HAIRLINE));
}

pub fn two_lines(canvas: &mut Canvas, left: f32, right: f32, center_y: f32, lines: &Lines<'_>) {
    let room = right - left;
    let title = controls::truncate(canvas, lines.title, font::body(), room);
    let (title_y, detail_y) =
        if lines.detail.is_empty() { (center_y, center_y) } else { (center_y - LINE_OFFSET, center_y + LINE_OFFSET) };
    if let Some(Lit::Title(start, end)) = lines.lit {
        lit_band(canvas, pos2(left, title_y), (lines.title, &title), start..end, font::body());
    }
    canvas.text(pos2(left, title_y), Align2::LEFT_CENTER, title, font::body(), label());
    if !lines.detail.is_empty() {
        let detail = match lines.cut {
            Cut::End => controls::truncate(canvas, lines.detail, font::caption(), room),
            Cut::Start => controls::truncate_start(canvas, lines.detail, font::caption(), room),
        };
        if let Some(Lit::Detail(start, end)) = lines.lit {
            lit_band(canvas, pos2(left, detail_y), (lines.detail, &detail), start..end, font::caption());
        }
        canvas.text(pos2(left, detail_y), Align2::LEFT_CENTER, detail, font::caption(), lines.tone);
    }
}

/// The band behind a lit run, there only while the run is still whole in the shown (perhaps cut) text.
fn lit_band(canvas: &mut Canvas, left_center: Pos2, (whole, shown): (&str, &str), run: Range<usize>, font: FontId) {
    let kept = run.end <= shown.len()
        && shown.is_char_boundary(run.start)
        && shown.is_char_boundary(run.end)
        && whole.get(..run.end) == shown.get(..run.end);
    if !kept || run.is_empty() {
        return;
    }
    // A trailing space has no width on its own, so each edge is measured with a letter after it.
    let probe = canvas.text_width(LIT_PROBE, font.clone());
    let edge = |text: &str, canvas: &Canvas| canvas.text_width(format!("{text}{LIT_PROBE}"), font.clone()) - probe;
    let from = edge(&shown[..run.start], canvas);
    let to = edge(&shown[..run.end], canvas);
    let h = font.size + 2.0 * LIT_PAD;
    let band = Rect::from_min_size(
        pos2(left_center.x + from - LIT_PAD, left_center.y - h * 0.5),
        vec2(to - from + 2.0 * LIT_PAD, h),
    );
    canvas.rect(band, LIT_RADIUS, white(LIT_ALPHA));
}

pub fn switch_row(
    ui: &egui::Ui,
    canvas: &mut Canvas,
    rect: Rect,
    id: Id,
    lines: &Lines<'_>,
    on: bool,
    live: bool,
) -> bool {
    let at = Rect::from_center_size(pos2(rect.right() - INSET - SWITCH.x * 0.5, rect.center().y), SWITCH);
    two_lines(canvas, rect.left() + INSET, at.left() - TEXT_GAP, rect.center().y, lines);
    switch(ui, canvas, at, id, on, live)
}

#[cfg(test)]
mod tests;
