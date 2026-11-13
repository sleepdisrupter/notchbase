//! The tiles: a scrolling strip of the shelf's files with thumbnails, captions and remove badges.

use std::time::{Duration, Instant};

use egui::epaint::RectShape;
use egui::{Align2, Color32, CornerRadius, Id, Pos2, Rect, Sense, Stroke, TextureHandle, Vec2, pos2, vec2};

use crate::bridge::Action;
use crate::pages::PageCtx;
use crate::shelf::{Item, ItemId, Kind, Origin};
use crate::ui::canvas::{Canvas, fade, scale_about};
use crate::ui::color::{self, label, secondary_label, white};
use crate::ui::glass::{self, Glass};
use crate::ui::glyph::{self, Glyph};
use crate::ui::spring::{Spring, SpringConfig, UNIT_PRECISION, lerp, smoothstep};
use crate::ui::{controls, font, rows};

use super::effects::{self, Content};
use super::icons::{document_icon, folder_icon};
use super::{ShelfPage, human_bytes, human_duration};

pub(super) const SCROLL: SpringConfig = SpringConfig::new(0.30, 1.0);
const COLUMNS: usize = 5;
const TILE_GAP: f32 = 6.0;
const WELL: f32 = 52.0;
const WELL_RADIUS: f32 = 12.0;
const NAME_GAP: f32 = 5.0;
const NAME_H: f32 = 15.0;
pub(super) const TILES_H: f32 = WELL + NAME_GAP + NAME_H;
const ICON: f32 = 36.0;
const SELECTED_WASH: f32 = 0.14;
const HOVER_WASH: f32 = 0.06;
const BADGE: f32 = 17.0;
const BADGE_SLOP: f32 = 2.0;
const THUMB_RADIUS: u8 = 6;
const PILL_H: f32 = 12.0;
const ROW_SLOP: f32 = 6.0;
const WHEEL_STEP: f32 = 48.0;
const EDGE_FADE: f32 = 1.6;
const POP_FROM: f32 = 0.55;
const POP: SpringConfig = SpringConfig::new(0.42, 0.62);
const FRESH: Duration = Duration::from_millis(1500);
const LEAVING_FROM: f32 = 0.7;
const FULL_UV: Rect = Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0));

struct Strip {
    row: Rect,
    tile_w: f32,
    pitch: f32,
}

impl Strip {
    fn new(row: Rect) -> Self {
        let tile_w = (row.width() - TILE_GAP * (COLUMNS - 1) as f32) / COLUMNS as f32;
        Self { row, tile_w, pitch: tile_w + TILE_GAP }
    }

    fn max_scroll(&self, slots: f32) -> f32 {
        let content_w = slots * self.pitch - TILE_GAP;
        (content_w - self.row.width()).max(0.0)
    }

    fn tile(&self, along: f32, offset: f32) -> Rect {
        let x = self.row.left() + along * self.pitch - offset;
        Rect::from_min_size(pos2(x, self.row.top()), vec2(self.tile_w, TILES_H))
    }

    fn visibility(&self, tile: Rect) -> f32 {
        let overflow = (self.row.left() - tile.left()).max(tile.right() - self.row.right()).max(0.0);
        1.0 - smoothstep(0.0, self.tile_w * EDGE_FADE, overflow)
    }
}

struct Slot {
    well: Rect,
    badge: Rect,
    body: Rect,
}

impl Slot {
    fn new(tile: Rect) -> Self {
        let well = Rect::from_center_size(pos2(tile.center().x, tile.top() + WELL * 0.5), Vec2::splat(WELL));
        let badge = Rect::from_center_size(well.left_top() + vec2(3.0, 3.0), Vec2::splat(BADGE));
        let caption = Rect::from_min_size(pos2(tile.left(), well.bottom()), vec2(tile.width(), NAME_GAP + NAME_H));
        Self { well, badge, body: well.union(caption) }
    }

    fn badge_target(&self) -> Rect {
        self.badge.expand(BADGE_SLOP)
    }
}

struct Shown<'a> {
    tile: Rect,
    item: &'a Item,
}

#[derive(Clone, Copy, Default)]
struct TilePointer {
    hovered: bool,
    remove_hovered: bool,
}

impl ShelfPage {
    pub(super) fn tiles_ui(
        &mut self,
        ctx: &mut PageCtx<'_>,
        row: Rect,
        items: &[Item],
        interactive: bool,
        actions: &mut Vec<Action>,
    ) {
        let strip = Strip::new(row);
        let laid: Vec<(ItemId, f32)> = self.tiles.iter().map(|(id, _, shown)| (*id, shown)).collect();
        let slots: f32 = laid.iter().map(|(_, shown)| shown).sum();
        let offset = self.scroll_offset(ctx.ui, row, strip.max_scroll(slots), interactive, ctx.dt);
        self.adopt_new_drops(items, ctx.now);
        let mut along = 0.0;
        for (id, room) in laid {
            let tile = strip.tile(along, offset);
            along += room;
            let visible = strip.visibility(tile);
            if visible <= 0.01 {
                continue;
            }
            let gone = items.iter().all(|item| item.id != id).then(|| self.leaving_item(id)).flatten();
            let Some(item) = items.iter().find(|item| item.id == id).or(gone.as_ref()) else { continue };
            let leaving = gone.is_some();
            let pop = if leaving { lerp(LEAVING_FROM, 1.0, room) } else { self.pop(item, ctx.now, ctx.dt) };
            let flight = self.liftoff.pose(item.id, tile, row);
            let live = interactive && !leaving && visible > 0.9 && flight.is_none();
            let mark = ctx.canvas.mark();
            let shown = Shown { tile, item };
            self.tile_ui(ctx.ui, ctx.canvas, &shown, live, actions);
            let scale = pop.max(0.0);
            let grow = scale_about(tile.center(), scale);
            let presence = if leaving { rows::alpha(room) } else { smoothstep(POP_FROM, 0.85, pop) };
            let mut alpha = visible * presence;
            let lift = flight.map(|pose| {
                alpha *= pose.alpha;
                pose.transform(tile)
            });
            for shape in ctx.canvas.since(mark) {
                if scale != 1.0 {
                    shape.transform(grow);
                }
                if let Some(lift) = lift {
                    shape.transform(lift);
                }
                fade(shape, alpha);
            }
        }
    }

    fn leaving_item(&self, id: ItemId) -> Option<Item> {
        self.tiles.iter().find(|(key, _, _)| **key == id).map(|(_, item, _)| item.clone())
    }

    fn scroll_offset(&mut self, ui: &egui::Ui, row: Rect, max_scroll: f32, interactive: bool, dt: f32) -> f32 {
        let hovered = ui.rect_contains_pointer(row.expand2(vec2(0.0, ROW_SLOP)));
        if interactive && hovered && max_scroll > 0.0 {
            let notches = crate::ui::scroll::across(ui);
            if notches != 0.0 {
                let target = (self.scroll.target - notches * WHEEL_STEP).clamp(0.0, max_scroll);
                self.scroll.animate_to(target, SCROLL);
            }
        }
        if self.scroll.target > max_scroll {
            self.scroll.animate_to(max_scroll, SCROLL);
        }
        self.scroll.step(dt);
        self.scroll.value
    }

    fn adopt_new_drops(&mut self, items: &[Item], now: Instant) {
        let dropped: Vec<ItemId> = items
            .iter()
            .filter(|item| !self.known.contains(&item.id) && item.origin == Origin::Dropped)
            .filter(|item| now.saturating_duration_since(item.added) < FRESH)
            .map(|item| item.id)
            .collect();
        if !dropped.is_empty() {
            self.selected = dropped;
            self.formats_open = false;
        }
    }

    fn pop(&mut self, item: &Item, now: Instant, dt: f32) -> f32 {
        if self.known.insert(item.id) && now.saturating_duration_since(item.added) < FRESH {
            let mut spring = Spring::new(POP_FROM, POP, UNIT_PRECISION);
            spring.animate_to(1.0, POP);
            self.pops.insert(item.id, spring);
        }
        let Some(spring) = self.pops.get_mut(&item.id) else { return 1.0 };
        spring.step(dt);
        let value = spring.value;
        if spring.is_settled() {
            self.pops.remove(&item.id);
        }
        value
    }

    fn tile_ui(
        &mut self,
        ui: &egui::Ui,
        canvas: &mut Canvas,
        shown: &Shown,
        interactive: bool,
        actions: &mut Vec<Action>,
    ) {
        let Shown { tile, item } = *shown;
        let id = Id::new(("notchbase.shelf.tile", item.id.0));
        let slot = Slot::new(tile);
        let selected = self.selected.contains(&item.id);
        let pointer =
            if interactive { self.tile_input(ui, id, &slot, item.id, actions) } else { TilePointer::default() };
        let hover = controls::animate(ui, id.with("hover"), pointer.hovered);
        let sel = controls::animate(ui, id.with("selected"), selected);

        canvas.rect(slot.well, WELL_RADIUS, white(SELECTED_WASH * sel + HOVER_WASH * hover * (1.0 - sel)));
        let content = self.paint_well(canvas, slot.well, item, hover);
        if matches!(item.kind, Kind::Video | Kind::Audio)
            && let Some(seconds) = item.seconds
        {
            paint_duration(canvas, content.rect, seconds);
        }
        if let Some(work) = self.work.tile(item.id) {
            effects::paint_work(canvas, content, work, self.clock);
        }
        paint_caption(canvas, tile, slot.well, item, hover, sel);

        let badge = controls::animate(ui, id.with("badge"), pointer.hovered);
        let pressing = controls::animate(ui, id.with("remove"), pointer.remove_hovered);
        paint_remove_badge(canvas, slot.badge, badge, pressing);
    }

    fn tile_input(
        &mut self,
        ui: &egui::Ui,
        id: Id,
        slot: &Slot,
        item: ItemId,
        actions: &mut Vec<Action>,
    ) -> TilePointer {
        let response = ui.interact(slot.body, id, Sense::click_and_drag());
        let hovered = response.hovered() || response.dragged() || ui.rect_contains_pointer(slot.badge_target());
        if response.drag_started() {
            actions.push(Action::DragOut(self.drag_selection(item)));
        } else if response.double_clicked() {
            actions.push(Action::ShelfReveal(item));
        } else if response.clicked() {
            self.toggle(item);
        }
        let mut remove_hovered = false;
        if hovered {
            let remove = ui.interact(slot.badge_target(), id.with("remove"), Sense::click());
            remove_hovered = remove.hovered();
            if remove.clicked() {
                actions.push(Action::ShelfRemove(item));
            }
        }
        TilePointer { hovered, remove_hovered }
    }

    fn drag_selection(&self, item: ItemId) -> Vec<ItemId> {
        if self.selected.contains(&item) { self.selected.clone() } else { vec![item] }
    }

    fn paint_well(&self, canvas: &mut Canvas, well: Rect, item: &Item, hover: f32) -> Content {
        match self.thumbs.get(&item.id) {
            Some(thumb) => paint_thumbnail(canvas, well, &thumb.texture, item.kind, hover),
            None => paint_placeholder(canvas, well, item),
        }
    }
}

fn paint_thumbnail(canvas: &mut Canvas, well: Rect, texture: &TextureHandle, kind: Kind, hover: f32) -> Content {
    let [tw, th] = texture.size();
    let aspect = tw as f32 / th.max(1) as f32;
    let max = WELL - 4.0 + 2.0 * hover;
    let size = if aspect >= 1.0 { vec2(max, max / aspect) } else { vec2(max * aspect, max) };
    let rect = Rect::from_center_size(well.center(), size);
    let rounded = matches!(kind, Kind::Image | Kind::Video);
    let radius = if rounded { THUMB_RADIUS } else { 0 };
    canvas.add(RectShape::filled(rect, CornerRadius::same(radius), Color32::WHITE).with_texture(texture.id(), FULL_UV));
    if rounded {
        canvas.rect_stroke(rect, f32::from(radius), Stroke::new(1.0, white(0.14)));
    }
    Content { rect, radius: f32::from(radius) }
}

fn paint_placeholder(canvas: &mut Canvas, well: Rect, item: &Item) -> Content {
    match item.kind {
        Kind::Folder => folder_icon(canvas, well.center(), ICON),
        kind => document_icon(canvas, well.center(), ICON, &item.ext, kind),
    }
    Content { rect: well, radius: WELL_RADIUS }
}

fn paint_duration(canvas: &mut Canvas, content: Rect, seconds: f64) {
    let text = human_duration(seconds);
    let width = canvas.text_width(text.as_str(), font::caption()) + 8.0;
    let pill = Rect::from_min_size(content.left_bottom() + vec2(3.0, -3.0 - PILL_H), vec2(width, PILL_H));
    canvas.rect(pill, PILL_H * 0.5, Color32::from_black_alpha(150));
    canvas.text(pill.center(), Align2::CENTER_CENTER, text, font::caption(), Color32::WHITE);
}

fn paint_caption(canvas: &mut Canvas, tile: Rect, well: Rect, item: &Item, hover: f32, sel: f32) {
    let at = pos2(tile.center().x, well.bottom() + NAME_GAP + NAME_H * 0.5);
    let name = controls::truncate(canvas, &item.name, font::subheadline(), tile.width() - 6.0);
    let details = if hover > 0.001 { details(item) } else { String::new() };
    let swap = if details.is_empty() { 0.0 } else { hover };
    let name_color = color::mix(secondary_label(), white(1.0), sel).gamma_multiply(1.0 - swap);
    canvas.text(at, Align2::CENTER_CENTER, name, font::subheadline(), name_color);
    if swap > 0.001 {
        let details = controls::truncate(canvas, &details, font::subheadline(), tile.width() - 2.0);
        canvas.text(at, Align2::CENTER_CENTER, details, font::subheadline(), secondary_label().gamma_multiply(swap));
    }
}

fn paint_remove_badge(canvas: &mut Canvas, badge: Rect, shown: f32, hovered: f32) {
    if shown <= 0.001 {
        return;
    }
    let center = badge.center();
    let r = BADGE * 0.5 * lerp(0.6, 1.0, shown);
    let glass = Glass::lerp(Glass::CONTROL, Glass::TILE_HOVER, hovered);
    let mark = canvas.mark();
    glass::paint(canvas, Rect::from_center_size(center, Vec2::splat(2.0 * r)), r, glass);
    glyph::paint(canvas, Glyph::Xmark, center, r * 1.05, label());
    for shape in canvas.since(mark) {
        fade(shape, shown);
    }
}

fn details(item: &Item) -> String {
    match (item.kind, item.dims, item.pages) {
        (Kind::Image, Some([w, h]), _) => format!("{w}×{h}"),
        (Kind::Pdf, _, Some(1)) => "1 page".to_owned(),
        (Kind::Pdf, _, Some(n)) => format!("{n} pages"),
        (Kind::Video, Some([w, h]), _) => format!("{w}×{h}"),
        _ => item.bytes.map(human_bytes).unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn item(id: u64, origin: Origin, added: Instant) -> Item {
        Item {
            id: ItemId(id),
            path: PathBuf::new(),
            name: format!("{id}.png"),
            kind: Kind::Image,
            ext: "PNG".into(),
            bytes: None,
            pages: None,
            seconds: None,
            dims: None,
            thumb: None,
            thumb_rev: 0,
            added,
            origin,
        }
    }

    #[test]
    fn dropped_files_pop_in_selected() {
        let now = Instant::now();
        let mut page = ShelfPage::default();
        let items = [item(1, Origin::Dropped, now), item(2, Origin::Produced, now)];
        page.adopt_new_drops(&items, now);
        assert_eq!(page.selected, [ItemId(1)]);
        assert!(page.pop(&items[0], now, 0.0) < 1.0);
        let stale = [item(3, Origin::Dropped, now - FRESH)];
        page.adopt_new_drops(&stale, now);
        assert_eq!(page.selected, [ItemId(1)]);
    }

    fn shelf_of(names: &[&str]) -> crate::shelf::Shelf {
        let mut shelf = crate::shelf::Shelf::new(std::sync::Arc::new(|| {}));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        shelf.add(names.iter().map(|name| root.join(name)).collect());
        shelf
    }

    fn laid(page: &ShelfPage) -> Vec<(ItemId, f32)> {
        page.tiles.iter().map(|(id, _, shown)| (*id, shown)).collect()
    }

    fn frames(
        harness: &mut crate::pages::harness::Harness,
        page: &mut ShelfPage,
        shelf: &crate::shelf::Shelf,
        n: usize,
    ) {
        for _ in 0..n {
            harness.frame(Vec::new(), |ctx| page.ui(ctx, shelf, None));
        }
    }

    #[test]
    fn removed_tile_folds_away() {
        let mut harness = crate::pages::harness::Harness::new();
        let (mut page, mut shelf) = (ShelfPage::default(), shelf_of(&["Cargo.toml", "Cargo.lock", "rustfmt.toml"]));
        frames(&mut harness, &mut page, &shelf, 3);
        let first = shelf.items()[0].id;
        assert!(laid(&page).iter().all(|(_, shown)| *shown == 1.0), "tiles already there are simply there");
        shelf.remove(first);
        frames(&mut harness, &mut page, &shelf, 2);
        let leaving = laid(&page);
        assert_eq!(leaving.len(), 3, "the removed tile is still drawn while it folds");
        assert!(leaving[0].0 == first && leaving[0].1 > 0.0 && leaving[0].1 < 1.0, "{leaving:?}");
        assert!(page.thumbs.len() <= 3 && page.is_animating());
        frames(&mut harness, &mut page, &shelf, 60);
        assert_eq!(laid(&page).len(), 2);
    }

    #[test]
    fn clearing_fades_into_empty() {
        let mut harness = crate::pages::harness::Harness::new();
        let (mut page, mut shelf) = (ShelfPage::default(), shelf_of(&["Cargo.toml", "Cargo.lock"]));
        frames(&mut harness, &mut page, &shelf, 3);
        shelf.clear();
        frames(&mut harness, &mut page, &shelf, 2);
        assert!(page.tiles.most_shown() > 0.0, "the tiles fold away instead of vanishing");
        assert!(page.status.previous().is_some() && page.buttons.previous().is_some(), "the rows below fade too");
        frames(&mut harness, &mut page, &shelf, 60);
        assert!(page.tiles.is_empty() && !page.is_animating());
    }
}
