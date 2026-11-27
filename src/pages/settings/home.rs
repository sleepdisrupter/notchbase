//! The Settings home: a search field over a grid of tiles, one per group. Typing turns the grid into the settings
//! that match, with chips to narrow them to one group.

use egui::{Align2, Id, Pos2, Rect, Sense, Stroke, Vec2, pos2, vec2};

use super::editing::Editing;
use super::group::Group;
use super::item::Went;
use super::rows::{INSET, INTERACTIVE_FROM, ROW_H, Row, card, paint_lines};
use super::search::{self, Found, Search, Spot};
use crate::bridge::{Action, PanelData};
use crate::pages::PageCtx;
use crate::ui::canvas::Canvas;
use crate::ui::color::{self, INK, label, secondary_label, white};
use crate::ui::field::bare::{Bare, bare};
use crate::ui::field::{self, Edit};
use crate::ui::glass::{self, Glass};
use crate::ui::glyph::{self, Glyph};
use crate::ui::list::TEXT_GAP;
use crate::ui::metrics::CARD_RADIUS;
use crate::ui::{controls, font};

const FIELD_H: f32 = 40.0;
const FIELD_ID: &str = "notchbase.settings.search";
const MAGNIFIER: f32 = 16.0;
const TEXT_SIZE: f32 = 15.0;
const CLEAR_R: f32 = 9.0;
const CLEAR_GLYPH: f32 = 7.0;
const RING: f32 = 1.25;
const RING_ALPHA: f32 = 0.32;
const BELOW_FIELD: f32 = 12.0;
const COLUMNS: usize = 2;
const TILE_H: f32 = 62.0;
const TILE_GAP: f32 = 8.0;
const TILE_PAD: f32 = 12.0;
const WELL: f32 = 32.0;
const WELL_ALPHA: f32 = 0.09;
const WELL_GLYPH: f32 = 15.0;
const BADGE_R: f32 = 4.5;
const QUIT: &str = "Quit notchbase";
const QUIT_ALPHA: f32 = 0.1;
const CHIP_H: f32 = 26.0;
const CHIP_PAD: f32 = 12.0;
const CHIP_GLYPH: f32 = 11.0;
const CHIP_GAP: f32 = 6.0;
const CHIP_PICKED: f32 = 0.85;
const PLACEHOLDER: &str = "Search settings";

/// Where the home view asks to go.
pub(super) enum Go {
    Group(Group),
    Went(Went),
}

pub(super) fn height(search: &Search, data: &PanelData, width: f32, canvas: &Canvas) -> f32 {
    let below = if search.is_on() {
        let found = search::find(&search.query, data);
        let chips = chips_height(&laid_chips(canvas, &found, pos2(0.0, 0.0), width));
        chips + BELOW_FIELD + shown(&found, search.only).len().max(1) as f32 * ROW_H
    } else {
        grid_height(cells(Group::tiles().len()))
    };
    FIELD_H + BELOW_FIELD + below
}

/// The tiles and the Quit tile after them.
fn cells(tiles: usize) -> usize {
    tiles + 1
}

fn grid_height(cells: usize) -> f32 {
    let rows = cells.div_ceil(COLUMNS) as f32;
    rows * TILE_H + (rows - 1.0).max(0.0) * TILE_GAP
}

pub(super) fn ui(
    ctx: &mut PageCtx<'_>,
    data: &PanelData,
    search: &mut Search,
    editing: &mut Editing,
    actions: &mut Vec<Action>,
) -> Option<Go> {
    let live = ctx.alpha > INTERACTIVE_FROM;
    let rect = Rect::from_min_size(ctx.rect.min, vec2(ctx.rect.width(), FIELD_H));
    search_field(ctx, rect, search, editing, (live, actions));
    let top = rect.bottom() + BELOW_FIELD;
    if search.is_on() {
        return results(ctx, top, data, search, (live, editing), actions).map(Go::Went);
    }
    tiles(ctx, top, data, live, actions)
}

fn search_field(
    ctx: &mut PageCtx<'_>,
    rect: Rect,
    search: &mut Search,
    editing: &mut Editing,
    (live, actions): (bool, &mut Vec<Action>),
) {
    let id = Id::new(FIELD_ID);
    let focused = editing.focus.as_ref().is_some_and(|focus| focus.is(id));
    let response = ctx.ui.interact(rect, id, if live { Sense::click() } else { Sense::hover() });
    if live && response.clicked() && !focused {
        field::begin(&mut editing.focus, id, &search.query, rect, actions);
    }
    let focused = editing.focus.as_ref().is_some_and(|focus| focus.is(id));
    let hover = controls::animate(ctx.ui, id.with("hover"), live && response.hovered() && !focused);
    let radius = rect.height() * 0.5;
    glass::paint(ctx.canvas, rect, radius, Glass::lerp(Glass::CONTROL, Glass::TILE_HOVER, hover * 0.6));
    if focused {
        ctx.canvas.rect_stroke(rect, radius, Stroke::new(RING, white(RING_ALPHA)));
    }
    let typed = !search.query.is_empty();
    let magnifier = pos2(rect.left() + radius, rect.center().y);
    let tint = if typed || focused { label() } else { secondary_label() };
    glyph::paint(ctx.canvas, Glyph::Search, magnifier, MAGNIFIER, tint);
    let mut right = rect.right() - radius * 0.5;
    if typed {
        let disc = pos2(rect.right() - radius, rect.center().y);
        let hit = Rect::from_center_size(disc, Vec2::splat(2.0 * CLEAR_R + 6.0));
        let clear = ctx.ui.interact(hit, id.with("clear"), if live { Sense::click() } else { Sense::hover() });
        let lit = controls::animate(ctx.ui, id.with("clear.hover"), live && clear.hovered());
        ctx.canvas.circle(disc, CLEAR_R, white(0.2 + 0.12 * lit));
        glyph::paint(ctx.canvas, Glyph::Xmark, disc, CLEAR_GLYPH, label());
        if live && clear.clicked() {
            forget(search, editing);
        }
        right = disc.x - CLEAR_R - TEXT_GAP;
    }
    let text_left = magnifier.x + MAGNIFIER * 0.5 + 9.0;
    let text_font = font::regular(TEXT_SIZE);
    let placeholder = if search.query.is_empty() { PLACEHOLDER } else { "" };
    if !focused && typed {
        let shown = controls::truncate(ctx.canvas, &search.query, text_font.clone(), right - text_left);
        ctx.canvas.text(pos2(text_left, rect.center().y), Align2::LEFT_CENTER, shown, text_font.clone(), label());
    }
    let field = Bare { id, rect, text_left, text_right: right.max(text_left), placeholder, font: text_font };
    for edit in bare(ctx.ui, ctx.canvas, &field, &mut editing.focus) {
        match edit {
            Edit::Typed(text) => search.set(text),
            Edit::Submitted(_) => {
                field::release(&mut editing.focus, actions);
            }
            Edit::Cancelled if search.is_on() => forget(search, editing),
            Edit::Cancelled => {
                field::release(&mut editing.focus, actions);
            }
        }
    }
}

/// Empties the search, and the field too while it has the keyboard.
fn forget(search: &mut Search, editing: &mut Editing) {
    search.clear();
    if let Some(focus) = editing.focus.as_mut().filter(|focus| focus.is(Id::new(FIELD_ID))) {
        focus.set_text("");
    }
}

fn tiles(ctx: &mut PageCtx<'_>, top: f32, data: &PanelData, live: bool, actions: &mut Vec<Action>) -> Option<Go> {
    let page = ctx.rect;
    let width = (page.width() - TILE_GAP * (COLUMNS - 1) as f32) / COLUMNS as f32;
    let cell = |index: usize| {
        let (column, row) = (index % COLUMNS, index / COLUMNS);
        let at = pos2(page.left() + column as f32 * (width + TILE_GAP), top + row as f32 * (TILE_H + TILE_GAP));
        Rect::from_min_size(at, vec2(width, TILE_H))
    };
    let groups = Group::tiles();
    let mut go = None;
    for (index, group) in groups.iter().enumerate() {
        if tile(ctx, cell(index), *group, data, live) {
            go = match group {
                Group::Notifications => Some(Go::Went(Went::Notifications)),
                Group::Permissions => {
                    actions.push(Action::ShowPermissions);
                    None
                }
                group => Some(Go::Group(*group)),
            };
        }
    }
    let mut quit = cell(groups.len());
    if groups.len().is_multiple_of(COLUMNS) {
        quit.set_right(page.right());
    }
    if quit_tile(ctx, quit, live) {
        actions.push(Action::Quit);
    }
    go
}

fn tile(ctx: &mut PageCtx<'_>, rect: Rect, group: Group, data: &PanelData, live: bool) -> bool {
    let id = Id::new(("notchbase.settings.tile", group.title()));
    let response = ctx.ui.interact(rect, id, if live { Sense::click() } else { Sense::hover() });
    let hover = controls::animate(ctx.ui, id.with("hover"), live && response.hovered());
    glass::paint(ctx.canvas, rect, CARD_RADIUS, Glass::lerp(Glass::TILE, Glass::TILE_HOVER, hover));
    let well = pos2(rect.left() + TILE_PAD + WELL * 0.5, rect.center().y);
    ctx.canvas.circle(well, WELL * 0.5, white(WELL_ALPHA));
    glyph::paint(ctx.canvas, group.glyph(), well, WELL_GLYPH, label());
    if group == Group::Notifications && data.app.unread > 0 {
        let badge = well + vec2(WELL * 0.36, -WELL * 0.36);
        ctx.canvas.circle(badge, BADGE_R + 1.5, color::SURFACE);
        ctx.canvas.circle(badge, BADGE_R, color::RED);
    }
    let summary = group.summary(data);
    let row = Row { rect, title: group.title(), detail: &summary };
    let left = well.x + WELL * 0.5 + 10.0;
    // The rows' own painter keeps both lines to one width; here they start after the well.
    let text = Rect::from_min_max(pos2(left - INSET, rect.top()), rect.max);
    paint_lines(ctx, &Row { rect: text, ..row }, rect.right() - TILE_PAD + TEXT_GAP, secondary_label(), false);
    live && response.clicked()
}

fn quit_tile(ctx: &mut PageCtx<'_>, rect: Rect, live: bool) -> bool {
    let id = Id::new("notchbase.settings.quit");
    let response = ctx.ui.interact(rect, id, if live { Sense::click() } else { Sense::hover() });
    let hover = controls::animate(ctx.ui, id.with("hover"), live && response.hovered());
    ctx.canvas.rect_stroke(rect.shrink(0.5), CARD_RADIUS, Stroke::new(1.0, white(QUIT_ALPHA + 0.08 * hover)));
    let tint = color::mix(secondary_label(), label(), hover);
    ctx.canvas.text(rect.center(), Align2::CENTER_CENTER, QUIT, font::callout(), tint);
    live && response.clicked()
}

fn shown(found: &[Found], only: Option<Group>) -> Vec<&Found> {
    found.iter().filter(|found| only.is_none_or(|group| found.group == group)).collect()
}

fn results(
    ctx: &mut PageCtx<'_>,
    top: f32,
    data: &PanelData,
    search: &mut Search,
    (live, editing): (bool, &mut Editing),
    actions: &mut Vec<Action>,
) -> Option<Went> {
    let found = search::find(&search.query, data);
    let top = top + chips(ctx, top, &found, search, live) + BELOW_FIELD;
    let shown = shown(&found, search.only);
    if shown.is_empty() {
        let rect = card(ctx, top, 1)[0];
        let said = format!("No settings match \u{201c}{}\u{201d}", search.query.trim());
        let row = Row { rect, title: &said, detail: "" };
        paint_lines(ctx, &row, rect.right() - INSET, secondary_label(), false);
        return None;
    }
    let mut went = None;
    for (found, rect) in shown.iter().zip(card(ctx, top, shown.len())) {
        search::set_spot(ctx.ui, Spot { query: search.query.clone(), crumb: found.crumb.clone() });
        went = found.item.paint(ctx, rect, data, (live, editing), actions).or(went);
        search::clear_spot(ctx.ui);
    }
    went
}

struct Chip {
    group: Option<Group>,
    glyph: Glyph,
    text: String,
    title: &'static str,
}

/// "All" and then each group with something found, as many to a line as fit and the rest on the lines under them, so
/// every group can be picked.
fn laid_chips(canvas: &Canvas, found: &[Found], at: Pos2, width: f32) -> Vec<(Chip, Rect)> {
    let mut chips =
        vec![Chip { group: None, glyph: Glyph::Search, text: format!("All {}", found.len()), title: "All" }];
    for group in Group::tiles() {
        let count = found.iter().filter(|found| found.group == group).count();
        if count > 0 {
            let text = format!("{} {count}", group.title());
            chips.push(Chip { group: Some(group), glyph: group.glyph(), text, title: group.title() });
        }
    }
    let mut cursor = at;
    chips
        .into_iter()
        .map(|chip| {
            let size = vec2(canvas.text_width(&chip.text, font::callout()) + 2.0 * CHIP_PAD + CHIP_GLYPH + 5.0, CHIP_H);
            if cursor.x > at.x && cursor.x + size.x > at.x + width {
                cursor = pos2(at.x, cursor.y + CHIP_H + CHIP_GAP);
            }
            let rect = Rect::from_min_size(cursor, size);
            cursor.x = rect.right() + CHIP_GAP;
            (chip, rect)
        })
        .collect()
}

fn chips_height(chips: &[(Chip, Rect)]) -> f32 {
    let top = chips.first().map_or(0.0, |(_, rect)| rect.top());
    chips.iter().map(|(_, rect)| rect.bottom()).fold(top + CHIP_H, f32::max) - top
}

/// Paints the chips and returns how tall they are. A chip picks its group, again lets it go.
fn chips(ctx: &mut PageCtx<'_>, top: f32, found: &[Found], search: &mut Search, live: bool) -> f32 {
    let laid = laid_chips(ctx.canvas, found, pos2(ctx.rect.left(), top), ctx.rect.width());
    for (chip, rect) in &laid {
        let id = Id::new(("notchbase.settings.chip", chip.title));
        let response = ctx.ui.interact(*rect, id, if live { Sense::click() } else { Sense::hover() });
        let hover = controls::animate(ctx.ui, id.with("hover"), live && response.hovered());
        let picked = search.only == chip.group;
        let ink = if picked {
            ctx.canvas.rect(*rect, CHIP_H * 0.5, white(CHIP_PICKED));
            INK
        } else {
            glass::paint(ctx.canvas, *rect, CHIP_H * 0.5, Glass::lerp(Glass::CONTROL, Glass::TILE_HOVER, hover));
            label()
        };
        let glyph_at = pos2(rect.left() + CHIP_PAD + CHIP_GLYPH * 0.5 - 2.0, rect.center().y);
        glyph::paint(ctx.canvas, chip.glyph, glyph_at, CHIP_GLYPH, ink);
        let text_at = pos2(glyph_at.x + CHIP_GLYPH * 0.5 + 5.0, rect.center().y);
        ctx.canvas.text(text_at, Align2::LEFT_CENTER, chip.text.clone(), font::callout(), ink);
        if live && response.clicked() {
            search.only = if picked { None } else { chip.group };
        }
    }
    chips_height(&laid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_fits_quit_after_tiles() {
        assert_eq!(cells(11), 12);
        assert_eq!(grid_height(12), 6.0 * TILE_H + 5.0 * TILE_GAP);
        assert_eq!(grid_height(11), 6.0 * TILE_H + 5.0 * TILE_GAP, "an odd tile still takes a row");
        assert_eq!(grid_height(1), TILE_H);
    }
}
