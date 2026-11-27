//! The pieces every view is built from: cards, sections, rows, toggles, steppers and pills.

use egui::{Align2, Color32, Id, Rect, Sense, pos2, vec2};

use super::search;
use crate::pages::PageCtx;
use crate::ui::canvas::{Canvas, fade};
use crate::ui::color::{label, secondary_label};
use crate::ui::glass::{self, Glass};
use crate::ui::glyph::{self, Glyph};
use crate::ui::list::{self, Cut, Lines, TEXT_GAP};
use crate::ui::{controls, font};

pub(in crate::pages) use crate::ui::list::INSET;

pub(in crate::pages) const ROW_H: f32 = 50.0;
pub(in crate::pages) const BACK_H: f32 = 28.0;
const BACK_GLYPH: f32 = 10.0;
pub(in crate::pages) const SECTION_H: f32 = 26.0;
pub(super) const INTERACTIVE_FROM: f32 = 0.5;
const STEP: f32 = 24.0;
const STEP_GLYPH: f32 = 10.0;
const STEP_GAP: f32 = 6.0;
const STEP_VALUE_W: f32 = 58.0;
pub(super) const PILL_H: f32 = 24.0;
const PILL_PAD: f32 = 12.0;
const CHEVRON: f32 = 10.0;
const UNAVAILABLE: f32 = 0.45;
pub(super) const CHOICE: egui::Vec2 = vec2(186.0, 24.0);

pub(super) struct Row<'a> {
    pub(super) rect: Rect,
    pub(super) title: &'a str,
    pub(super) detail: &'a str,
}

pub(super) struct Toggle {
    pub(super) id: Id,
    pub(super) on: bool,
    pub(super) enabled: bool,
}

pub(in crate::pages) fn card(ctx: &mut PageCtx<'_>, top: f32, rows: usize) -> Vec<Rect> {
    card_of(ctx, top, &vec![ROW_H; rows])
}

pub(in crate::pages) fn card_of(ctx: &mut PageCtx<'_>, top: f32, heights: &[f32]) -> Vec<Rect> {
    let card = Rect::from_min_size(pos2(ctx.rect.left(), top), vec2(ctx.rect.width(), heights.iter().sum()));
    list::card(ctx.canvas, card, heights)
}

pub(in crate::pages) fn section(ctx: &mut PageCtx<'_>, top: f32, title: &str) -> Rect {
    let header = Rect::from_min_size(pos2(ctx.rect.left(), top), vec2(ctx.rect.width(), SECTION_H));
    controls::section_header(ctx.canvas, pos2(header.left() + INSET, header.center().y), title);
    header
}

/// A row's two lines. Painted as a search result, its detail starts with where it lives and the match is lit.
fn spotted_lines<R>(ctx: &mut PageCtx<'_>, row: &Row, paint: impl FnOnce(&mut PageCtx<'_>, Lines<'_>) -> R) -> R {
    let Some(spot) = search::spot(ctx.ui) else { return paint(ctx, Lines::new(row.title, row.detail)) };
    let detail = search::crumbed(&spot.crumb, row.detail);
    let lit = search::lit(&spot.query, row.title, &detail);
    paint(ctx, Lines::new(row.title, &detail).lit(lit))
}

pub(super) fn toggled(ctx: &mut PageCtx<'_>, row: &Row, toggle: &Toggle) -> bool {
    spotted_lines(ctx, row, |ctx, lines| toggled_with(ctx, row, toggle, &lines))
}

fn toggled_with(ctx: &mut PageCtx<'_>, row: &Row, toggle: &Toggle, lines: &Lines<'_>) -> bool {
    let unavailable = !toggle.enabled && ctx.alpha > INTERACTIVE_FROM;
    let dim = controls::animate(ctx.ui, toggle.id.with("unavailable"), unavailable);
    let mark = ctx.canvas.mark();
    let flipped = list::switch_row(ctx.ui, ctx.canvas, row.rect, toggle.id, lines, toggle.on, toggle.enabled);
    if dim > 0.0 {
        for shape in ctx.canvas.since(mark) {
            fade(shape, 1.0 - (1.0 - UNAVAILABLE) * dim);
        }
    }
    if flipped {
        crate::platform::haptic(crate::platform::Feel::Flip);
    }
    flipped
}

pub(super) fn paint_text(ctx: &mut PageCtx<'_>, row: &Row, control_left: f32) {
    paint_lines(ctx, row, control_left, secondary_label(), false);
}

pub(super) fn paint_lines(ctx: &mut PageCtx<'_>, row: &Row, control_left: f32, detail_tone: Color32, path: bool) {
    let cut = if path { Cut::Start } else { Cut::End };
    spotted_lines(ctx, row, |ctx, lines| {
        let lines = lines.tone(detail_tone).cut(cut);
        list::two_lines(ctx.canvas, row.rect.left() + INSET, control_left - TEXT_GAP, row.rect.center().y, &lines);
    });
}

pub(super) fn stepper(
    ctx: &mut PageCtx<'_>,
    row: &Row,
    id: Id,
    [value, min, max, step]: [f64; 4],
    unit: &str,
    live: bool,
) -> Option<f64> {
    let rect = row.rect;
    let plus =
        Rect::from_center_size(pos2(rect.right() - INSET - STEP * 0.5, rect.center().y), egui::Vec2::splat(STEP));
    let minus = plus.translate(vec2(-(STEP + STEP_GAP + STEP_VALUE_W + STEP_GAP), 0.0));
    paint_text(ctx, row, minus.left());
    let shown = if unit.is_empty() { trim(value) } else { format!("{} {unit}", trim(value)) };
    let center = pos2((minus.right() + plus.left()) * 0.5, rect.center().y);
    ctx.canvas.text(center, Align2::CENTER_CENTER, shown, font::value(), label());
    let mut next = None;
    for (button, glyph, delta) in [(minus, Glyph::Minus, -step), (plus, Glyph::Plus, step)] {
        if round_button(ctx, button, id.with(glyph as u8), glyph, live) {
            next = Some((value + delta).clamp(min, max));
        }
    }
    next.filter(|next| (next - value).abs() > f64::EPSILON)
}

pub(super) fn round_button_left_of(right: f32, center_y: f32) -> Rect {
    Rect::from_center_size(pos2(right - STEP_GAP - STEP * 0.5, center_y), egui::Vec2::splat(STEP))
}

pub(in crate::pages) fn round_button(ctx: &mut PageCtx<'_>, rect: Rect, id: Id, glyph: Glyph, live: bool) -> bool {
    let response = ctx.ui.interact(rect, id, if live { Sense::click() } else { Sense::hover() });
    let hover = controls::animate(ctx.ui, id.with("hover"), live && response.hovered());
    glass::paint(ctx.canvas, rect, rect.height() * 0.5, Glass::lerp(Glass::CONTROL, Glass::TILE_HOVER, hover));
    glyph::paint(ctx.canvas, glyph, rect.center(), STEP_GLYPH, label());
    live && response.clicked()
}

pub(super) fn pill_width(canvas: &Canvas, text: &str) -> f32 {
    canvas.text_width(text, font::callout()) + 2.0 * PILL_PAD
}

pub(in crate::pages) fn pill(
    ctx: &mut PageCtx<'_>,
    right: f32,
    center_y: f32,
    id: Id,
    text: &str,
    live: bool,
) -> (Rect, bool) {
    let width = pill_width(ctx.canvas, text);
    let rect = Rect::from_min_size(pos2(right - width, center_y - PILL_H * 0.5), vec2(width, PILL_H));
    let response = ctx.ui.interact(rect, id, if live { Sense::click() } else { Sense::hover() });
    let hover = controls::animate(ctx.ui, id.with("hover"), live && response.hovered());
    glass::paint(ctx.canvas, rect, PILL_H * 0.5, Glass::lerp(Glass::CONTROL, Glass::TILE_HOVER, hover));
    ctx.canvas.text(rect.center(), Align2::CENTER_CENTER, text, font::callout(), label());
    (rect, live && response.clicked())
}

pub(super) fn nav_row(ctx: &mut PageCtx<'_>, row: &Row, id: Id, glyph: Glyph, live: bool) -> bool {
    let response = ctx.ui.interact(row.rect, id, if live { Sense::click() } else { Sense::hover() });
    let hover = controls::animate(ctx.ui, id.with("hover"), live && response.hovered());
    controls::row_highlight(ctx.canvas, row.rect.shrink(3.0), hover);
    let at = pos2(row.rect.right() - INSET - CHEVRON * 0.5, row.rect.center().y);
    glyph::paint(ctx.canvas, glyph, at, CHEVRON, secondary_label());
    paint_text(ctx, row, at.x - CHEVRON * 0.5);
    live && response.clicked()
}

pub(in crate::pages) fn back(ctx: &mut PageCtx<'_>, top: f32, to: &str, live: bool) -> bool {
    let id = Id::new(("notchbase.settings.back", to));
    let font = font::section();
    let width = BACK_GLYPH + 6.0 + ctx.canvas.text_width(to, font.clone()) + 12.0;
    let rect = Rect::from_min_size(pos2(ctx.rect.left(), top), vec2(width, BACK_H));
    let response = ctx.ui.interact(rect, id, if live { Sense::click() } else { Sense::hover() });
    let hover = controls::animate(ctx.ui, id.with("hover"), live && response.hovered());
    controls::row_highlight(ctx.canvas, rect, hover);
    let tint = crate::ui::color::mix(secondary_label(), label(), hover);
    let glyph_at = pos2(rect.left() + 6.0 + BACK_GLYPH * 0.5, rect.center().y);
    glyph::paint(ctx.canvas, Glyph::ChevronLeft, glyph_at, BACK_GLYPH, tint);
    let text_at = pos2(glyph_at.x + BACK_GLYPH * 0.5 + 4.0, rect.center().y);
    ctx.canvas.text(text_at, Align2::LEFT_CENTER, to, font, tint);
    live && response.clicked()
}

pub(super) fn trim(value: f64) -> String {
    if value.fract().abs() < 1e-9 { format!("{}", value as i64) } else { format!("{value:.1}") }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pages::harness::Harness;

    #[test]
    fn unusable_toggle_dims() {
        let mut harness = Harness::new();
        let knob = |enabled: bool, harness: &mut Harness| {
            harness.frame(Vec::new(), |ctx| {
                let rect = Rect::from_min_size(ctx.rect.min, vec2(ctx.rect.width(), ROW_H));
                let row = Row { rect, title: "Automatic updates", detail: "" };
                let toggle = Toggle { id: Id::new("toggle"), on: true, enabled };
                toggled(ctx, &row, &toggle);
                let circles = ctx.canvas.shapes.iter().filter_map(|shape| match shape {
                    egui::Shape::Circle(circle) => Some(circle.fill.a()),
                    _ => None,
                });
                circles.max().unwrap_or(0)
            })
        };
        assert_eq!(knob(true, &mut harness), 255);
        let mut dimmed = 255;
        for _ in 0..30 {
            dimmed = knob(false, &mut harness);
        }
        assert!(dimmed < 140, "{dimmed}");
    }

    #[test]
    fn numbers_drop_needless_decimals() {
        assert_eq!(trim(25.0), "25");
        assert_eq!(trim(2.5), "2.5");
    }
}
