//! What the page shows instead of rows: Find off, starting up, no matches, or no access.

use egui::{Align2, Id, Pos2, Rect, Sense, Vec2, pos2, vec2};

use super::row::{INSET, LINE, TEXT_GAP, WELL};
use super::words;
use crate::bridge::{Access, Action, FindView};
use crate::pages::PageCtx;
use crate::pages::tools::well;
use crate::ui::canvas::fade;
use crate::ui::color::{self, label, secondary_label};
use crate::ui::glass::{self, Glass};
use crate::ui::glyph::{self, Glyph};
use crate::ui::{controls, font};

pub(super) const DISC: f32 = 22.0;
const DISC_GLYPH: f32 = 9.0;
const DISC_GAP: f32 = 6.0;
const OFF_SPREAD: f32 = 16.0;
const CAPSULE_H: f32 = 24.0;
const CAPSULE_PAD: f32 = 12.0;
const OFF: &str = "Find is off";
const TURN_ON: &str = "Turn On";
const LOOKING: &str = "Looking through your files";
const NO_MATCHES: &str = "No matches";
const NOT_INDEXED: &str = "Desktop, Documents and Downloads are not indexed yet";
const LEFT_OUT: &str = "Some folders were left out to save memory";
const SEE_ALL: &str = "See every file";
const SEE_ALL_DETAIL: &str = "Allow Full Disk Access to include Desktop, Documents and Downloads";
const SEE_ALL_SHORT: &str = "Full Disk Access adds Desktop, Documents and Downloads";
const ALLOW: &str = "Allow";
const ACCESS_ID: &str = "notchbase.find.access";
const TURN_ON_ID: &str = "notchbase.find.on";
const DETAIL_ROWS: usize = 2;

pub(super) fn off(ctx: &mut PageCtx<'_>, area: Rect, live: bool) -> Option<Action> {
    let middle = area.center();
    let title = pos2(middle.x, middle.y - OFF_SPREAD);
    ctx.canvas.text(title, Align2::CENTER_CENTER, OFF, font::headline(), label());
    let at = pos2(middle.x, middle.y + OFF_SPREAD);
    let (_, on) = capsule(ctx, at, Align2::CENTER_CENTER, Id::new(TURN_ON_ID), TURN_ON, live);
    on.then_some(Action::SetFindEnabled(true))
}

fn capsule(ctx: &mut PageCtx<'_>, at: Pos2, anchor: Align2, id: Id, text: &str, live: bool) -> (Rect, bool) {
    let width = ctx.canvas.text_width(text, font::callout()) + 2.0 * CAPSULE_PAD;
    let rect = anchor.anchor_size(at, vec2(width, CAPSULE_H));
    let response = ctx.ui.interact(rect, id, if live { Sense::click() } else { Sense::hover() });
    let hover = controls::animate(ctx.ui, id.with("hover"), live && response.hovered());
    glass::paint(ctx.canvas, rect, CAPSULE_H * 0.5, Glass::lerp(Glass::CONTROL, Glass::TILE_HOVER, hover));
    let tint = color::mix(label(), color::PEARL, hover);
    ctx.canvas.text(rect.center(), Align2::CENTER_CENTER, text, font::callout(), tint);
    (rect, live && response.clicked())
}

pub(super) fn starting(ctx: &mut PageCtx<'_>, area: Rect, entries: u32) {
    let middle = area.center();
    let count = words::items(entries);
    let title_at = if count.is_empty() { middle } else { pos2(middle.x, middle.y - LINE) };
    ctx.canvas.text(title_at, Align2::CENTER_CENTER, LOOKING, font::headline(), label());
    let at = pos2(middle.x, middle.y + LINE);
    ctx.canvas.text(at, Align2::CENTER_CENTER, count, font::subheadline(), secondary_label());
}

pub(super) fn none(ctx: &mut PageCtx<'_>, area: Rect, view: &FindView) {
    let caption = match (view.access, view.truncated) {
        (Access::Limited, _) => Some(NOT_INDEXED),
        (Access::Full, true) => Some(LEFT_OUT),
        (Access::Full, false) => None,
    };
    let middle = area.center();
    let title_at = if caption.is_some() { pos2(middle.x, middle.y - LINE) } else { middle };
    ctx.canvas.text(title_at, Align2::CENTER_CENTER, NO_MATCHES, font::headline(), label());
    if let Some(caption) = caption {
        let at = pos2(middle.x, middle.y + LINE);
        ctx.canvas.text(at, Align2::CENTER_CENTER, caption, font::subheadline(), secondary_label());
    }
}

pub(super) fn access(ctx: &mut PageCtx<'_>, rect: Rect, live: bool) -> Option<Action> {
    let center = pos2(rect.left() + INSET + WELL * 0.5, rect.center().y);
    well::paint(ctx.canvas, center, WELL, 0.0);
    well::glyph(ctx.canvas, Glyph::Lock, center, WELL, color::PEARL);
    let id = Id::new(ACCESS_ID);
    let right = rect.right() - INSET;
    let dismiss = Rect::from_center_size(pos2(right - DISC * 0.5, rect.center().y), Vec2::splat(DISC));
    let hide = disc(ctx, dismiss, id.with("hide"), live, 1.0);
    let at = pos2(dismiss.left() - DISC_GAP, rect.center().y);
    let (allow_rect, allow) = capsule(ctx, at, Align2::RIGHT_CENTER, id.with("allow"), ALLOW, live);
    let left = center.x + WELL * 0.5 + TEXT_GAP;
    let room = (allow_rect.left() - TEXT_GAP - left).max(1.0);
    let wrapped = |text: &str| ctx.canvas.wrapped(text, font::subheadline(), secondary_label(), room);
    let full = wrapped(SEE_ALL_DETAIL);
    let detail = if full.rows.len() <= DETAIL_ROWS { SEE_ALL_DETAIL } else { SEE_ALL_SHORT };
    let galley = ctx.canvas.clamped(detail, font::subheadline(), secondary_label(), room, DETAIL_ROWS);
    let block = font_height(ctx, font::headline()) + galley.size().y;
    let top = rect.center().y - block * 0.5;
    ctx.canvas.text(pos2(left, top), Align2::LEFT_TOP, SEE_ALL, font::headline(), label());
    ctx.canvas.add(egui::Shape::galley(pos2(left, top + font_height(ctx, font::headline())), galley, label()));
    allow.then_some(Action::AllowFullDiskAccess).or(hide.then_some(Action::FindHideAccess))
}

pub(super) fn disc(ctx: &mut PageCtx<'_>, rect: Rect, id: Id, live: bool, alpha: f32) -> bool {
    let response = ctx.ui.interact(rect, id, if live { Sense::click() } else { Sense::hover() });
    let hover = controls::animate(ctx.ui, id.with("hover"), live && response.hovered());
    let mark = ctx.canvas.mark();
    glass::paint(ctx.canvas, rect, rect.height() * 0.5, Glass::lerp(Glass::CONTROL, Glass::TILE_HOVER, hover));
    glyph::paint(ctx.canvas, Glyph::Xmark, rect.center(), DISC_GLYPH, color::mix(label(), color::PEARL, hover));
    ctx.canvas.since(mark).iter_mut().for_each(|shape| fade(shape, alpha));
    live && response.clicked()
}

fn font_height(ctx: &PageCtx<'_>, font: egui::FontId) -> f32 {
    ctx.canvas.text_size("Ag", font).y
}
