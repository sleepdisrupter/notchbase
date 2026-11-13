//! What takes up the disk, part by part, with the biggest items in each part.

use std::collections::HashMap;
use std::sync::Arc;

use egui::epaint::RectShape;
use egui::{
    Align2, Color32, ColorImage, Id, ImageData, Pos2, Rect, Sense, TextureFilter, TextureHandle, TextureOptions, pos2,
    vec2,
};

use super::format::format_bytes;
use crate::bridge::Action;
use crate::pages::PageCtx;
use crate::stats::{Item, Part, PartUsage, Resource, Snapshot};
use crate::ui::canvas::{Canvas, fade};
use crate::ui::glyph::{self, Glyph};
use crate::ui::spring::{POINT_PRECISION, Spring, UNIT_PRECISION};
use crate::ui::{color, controls, font, metrics, motion};

const ROW_H: f32 = 28.0;
const BAR_ROW_H: f32 = 22.0;
const BAR_H: f32 = 8.0;
const BAR_GAP: f32 = 1.5;
const PARTS_SHOWN: usize = 8;
const ITEMS_SHOWN: usize = 7;
const ROW_BLEED: f32 = 6.0;
const ICON: f32 = 20.0;
const ICON_RADIUS: f32 = 5.0;
const ICON_GLYPH: f32 = 15.0;
const ICON_GAP: f32 = 10.0;
const SWATCH: f32 = 9.0;
const TRAIL: f32 = 18.0;
const TRAIL_GLYPH: f32 = 11.0;
const TRAIL_GAP: f32 = 6.0;
const NAME_GAP: f32 = 12.0;
const SLIDE: f32 = 14.0;
const DIMMED: f32 = 0.3;
const INTERACTIVE_FROM: f32 = 0.5;
const SHADES: [f32; 8] = [0.92, 0.7, 0.54, 0.42, 0.32, 0.25, 0.19, 0.15];
const TITLE: &str = "Storage";
const MEASURING: &str = "Measuring…";
const REST: &str = "Everything else";

struct Slice {
    part: Part,
    size: u64,
    tone: Color32,
    opens: bool,
}

struct Icon {
    source: Arc<ColorImage>,
    texture: TextureHandle,
}

pub(super) struct StorageList {
    open: Spring,
    rows: Spring,
    enter: Spring,
    from: f32,
    chosen: Option<Part>,
    icons: HashMap<String, Icon>,
}

impl Default for StorageList {
    fn default() -> Self {
        Self {
            open: Spring::new(0.0, motion::SMOOTH, UNIT_PRECISION),
            rows: Spring::new(0.0, motion::SMOOTH, POINT_PRECISION),
            enter: Spring::new(1.0, motion::SMOOTH, UNIT_PRECISION),
            from: 1.0,
            chosen: None,
            icons: HashMap::new(),
        }
    }
}

impl StorageList {
    pub(super) fn height(&self) -> f32 {
        self.open.value.clamp(0.0, 1.0) * (metrics::SECTION_GAP + ROW_H + BAR_ROW_H + self.rows.value.max(0.0))
    }

    pub(super) fn is_animating(&self) -> bool {
        !self.open.is_settled() || !self.enter.is_settled() || !self.rows.is_settled()
    }

    pub(super) fn ui(&mut self, ctx: &mut PageCtx<'_>, stats: &Snapshot, top_left: Pos2, width: f32) -> Vec<Action> {
        let listed = stats.listing == Some(Resource::Disk);
        if !self.open.is_settled() || listed != (self.open.target > 0.5) {
            self.open.animate_to(if listed { 1.0 } else { 0.0 }, motion::SMOOTH);
            self.open.step(ctx.dt);
        }
        self.enter.step(ctx.dt);
        let slices = slices(stats);
        if self.chosen.is_some_and(|part| stats.storage.part(part).is_none_or(|usage| usage.items.is_empty())) {
            self.chosen = None;
        }
        let detail = self.chosen.and_then(|part| stats.storage.part(part));
        let count = match detail {
            Some(usage) => shown_items(usage).len() + usize::from(rest_of(usage) > 0),
            None => slices.len(),
        };
        let wanted = ROW_H * count as f32;
        if self.open.value <= 0.001 {
            self.rows.snap_to(wanted);
        }
        self.rows.animate_to(wanted, motion::SMOOTH);
        self.rows.step(ctx.dt);
        let room = self.height() - metrics::SECTION_GAP;
        if room <= 0.0 {
            return Vec::new();
        }
        let top = top_left + vec2(0.0, metrics::SECTION_GAP);
        let live = listed && ctx.alpha >= INTERACTIVE_FROM;
        let opened = self.open.value.clamp(0.0, 1.0);

        let mut actions = Vec::new();
        let head = Rect::from_min_size(top, vec2(width, ROW_H));
        let mark = ctx.canvas.mark();
        let back = head_row(ctx, stats, detail, head, live);
        let bar = Rect::from_min_size(head.left_bottom(), vec2(width, BAR_ROW_H));
        paint_bar(ctx.ui, ctx.canvas, bar, (&slices, stats), self.chosen);
        ctx.canvas.since(mark).iter_mut().for_each(|shape| fade(shape, opened));

        let list_top = bar.left_bottom();
        let list_room = room - ROW_H - BAR_ROW_H;
        let (slide, entered) = (self.from * (1.0 - self.enter.value) * SLIDE, self.enter.value.clamp(0.0, 1.0));
        let at = |index: usize| Rect::from_min_size(list_top + vec2(slide, ROW_H * index as f32), vec2(width, ROW_H));
        let reveal = |index: usize| ((list_room - ROW_H * index as f32) / ROW_H).clamp(0.0, 1.0) * entered;
        let mut go = back.then_some(None);
        match detail {
            Some(usage) => {
                let items = shown_items(usage);
                for (i, item) in items.iter().enumerate().filter(|(i, _)| reveal(*i) > 0.0) {
                    let mark = ctx.canvas.mark();
                    if self.item_row(ctx, item, at(i), live) {
                        actions.push(Action::RevealPath(item.path.clone()));
                    }
                    ctx.canvas.since(mark).iter_mut().for_each(|shape| fade(shape, reveal(i)));
                }
                if rest_of(usage) > 0 && reveal(items.len()) > 0.0 {
                    let mark = ctx.canvas.mark();
                    rest_row(ctx.canvas, at(items.len()), rest_of(usage));
                    ctx.canvas.since(mark).iter_mut().for_each(|shape| fade(shape, reveal(items.len())));
                }
                self.icons.retain(|path, _| usage.items.iter().any(|item| item.path.to_string_lossy() == *path));
            }
            None => {
                for (i, slice) in slices.iter().enumerate().filter(|(i, _)| reveal(*i) > 0.0) {
                    let mark = ctx.canvas.mark();
                    if part_row(ctx, slice, at(i), live) {
                        go = Some(Some(slice.part));
                    }
                    ctx.canvas.since(mark).iter_mut().for_each(|shape| fade(shape, reveal(i)));
                }
            }
        }
        if let Some(chosen) = go {
            self.step_to(chosen);
        }
        actions
    }

    fn step_to(&mut self, chosen: Option<Part>) {
        self.from = if chosen.is_some() { 1.0 } else { -1.0 };
        self.chosen = chosen;
        self.enter.snap_to(0.0);
        self.enter.animate_to(1.0, motion::SMOOTH);
    }

    fn item_row(&mut self, ctx: &mut PageCtx<'_>, item: &Item, row: Rect, live: bool) -> bool {
        let lit = row.expand2(vec2(ROW_BLEED, 0.0));
        let id = Id::new("notchbase.stats.storage.item").with(&item.path);
        let response = live.then(|| ctx.ui.interact(lit, id, Sense::click()));
        let hover = controls::animate(ctx.ui, id.with("hover"), response.as_ref().is_some_and(|r| r.hovered()));
        controls::row_highlight(ctx.canvas, lit, hover);
        let icon = Rect::from_center_size(icon_center(row), vec2(ICON, ICON));
        match &item.icon {
            Some(source) => self.paint_icon(ctx.ui.ctx(), ctx.canvas, icon, item, source),
            None => {
                let mark = if item.folder { Glyph::Folder } else { Glyph::Document };
                glyph::paint(ctx.canvas, mark, icon.center(), ICON_GLYPH, color::secondary_label());
            }
        }
        let c = &mut *ctx.canvas;
        let size = c.text(
            value_at(row),
            Align2::RIGHT_CENTER,
            format_bytes(item.size),
            font::body(),
            color::secondary_label(),
        );
        let name = controls::truncate(c, &item.name, font::body(), size.left() - NAME_GAP - name_at(row).x);
        c.text(name_at(row), Align2::LEFT_CENTER, name, font::body(), color::label());
        response.is_some_and(|r| r.clicked())
    }

    fn paint_icon(&mut self, ctx: &egui::Context, c: &mut Canvas, rect: Rect, item: &Item, source: &Arc<ColorImage>) {
        let key = item.path.to_string_lossy().into_owned();
        if self.icons.get(&key).is_none_or(|icon| !Arc::ptr_eq(&icon.source, source)) {
            let options = TextureOptions::LINEAR.with_mipmap_mode(Some(TextureFilter::Linear));
            let name = format!("stats-storage-icon-{key}");
            let texture = ctx.load_texture(name, ImageData::Color(Arc::clone(source)), options);
            self.icons.insert(key.clone(), Icon { source: Arc::clone(source), texture });
        }
        let uv = Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0));
        c.add(RectShape::filled(rect, ICON_RADIUS, color::white(1.0)).with_texture(self.icons[&key].texture.id(), uv));
    }
}

fn head_row(ctx: &mut PageCtx<'_>, stats: &Snapshot, detail: Option<&PartUsage>, row: Rect, live: bool) -> bool {
    let Some(usage) = detail else {
        let c = &mut *ctx.canvas;
        c.text(row.left_center(), Align2::LEFT_CENTER, TITLE, font::headline(), color::label());
        let status = if stats.storage.measuring { MEASURING.to_owned() } else { summary(stats) };
        c.text(row.right_center(), Align2::RIGHT_CENTER, status, font::body(), color::tertiary_label());
        return false;
    };
    let id = Id::new("notchbase.stats.storage.head");
    let lit = row.expand2(vec2(ROW_BLEED, 0.0));
    let back = live.then(|| ctx.ui.interact(lit, id, Sense::click()));
    let hover = controls::animate(ctx.ui, id.with("hover"), back.as_ref().is_some_and(|r| r.hovered()));
    let c = &mut *ctx.canvas;
    controls::row_highlight(c, lit, hover);
    let chevron = color::mix(color::secondary_label(), color::label(), hover);
    glyph::paint(c, Glyph::ChevronLeft, icon_center(row), ICON_GLYPH, chevron);
    c.text(name_at(row), Align2::LEFT_CENTER, usage.part.title(), font::headline(), color::label());
    c.text(value_at(row), Align2::RIGHT_CENTER, format_bytes(usage.size), font::body(), color::secondary_label());
    back.is_some_and(|r| r.clicked())
}

fn part_row(ctx: &mut PageCtx<'_>, slice: &Slice, row: Rect, live: bool) -> bool {
    let lit = row.expand2(vec2(ROW_BLEED, 0.0));
    let id = Id::new("notchbase.stats.storage.part").with(slice.part as u8);
    let response = (live && slice.opens).then(|| ctx.ui.interact(lit, id, Sense::click()));
    let hover = controls::animate(ctx.ui, id.with("hover"), response.as_ref().is_some_and(|r| r.hovered()));
    let c = &mut *ctx.canvas;
    controls::row_highlight(c, lit, hover);
    c.circle(icon_center(row), SWATCH * 0.5, slice.tone);
    c.text(name_at(row), Align2::LEFT_CENTER, slice.part.title(), font::body(), color::label());
    c.text(value_at(row), Align2::RIGHT_CENTER, format_bytes(slice.size), font::body(), color::secondary_label());
    if slice.opens {
        let tint = color::mix(color::tertiary_label(), color::label(), hover);
        glyph::paint(c, Glyph::ChevronRight, pos2(row.right() - TRAIL * 0.5, row.center().y), TRAIL_GLYPH, tint);
    }
    response.is_some_and(|r| r.clicked())
}

fn rest_row(c: &mut Canvas, row: Rect, size: u64) {
    glyph::paint(c, Glyph::Layers, icon_center(row), ICON_GLYPH - 2.0, color::tertiary_label());
    c.text(name_at(row), Align2::LEFT_CENTER, REST, font::body(), color::secondary_label());
    c.text(value_at(row), Align2::RIGHT_CENTER, format_bytes(size), font::body(), color::tertiary_label());
}

fn paint_bar(ui: &egui::Ui, c: &mut Canvas, row: Rect, (slices, stats): (&[Slice], &Snapshot), chosen: Option<Part>) {
    let track = Rect::from_center_size(row.center(), vec2(row.width(), BAR_H));
    c.rect(track, BAR_H * 0.5, color::quaternary_label());
    let whole = stats.disk_total.unwrap_or(0).max(slices.iter().map(|slice| slice.size).sum()).max(1);
    let mut left = track.left();
    for slice in slices {
        let width = track.width() * (slice.size as f64 / whole as f64) as f32;
        let piece =
            Rect::from_min_max(pos2(left, track.top()), pos2(left + (width - BAR_GAP).max(0.0), track.bottom()));
        left += width;
        if piece.width() < 1.0 {
            continue;
        }
        let aside = chosen.is_some_and(|part| part != slice.part);
        let dim =
            1.0 - (1.0 - DIMMED) * controls::animate(ui, egui::Id::new(("notchbase.stats.aside", slice.part)), aside);
        c.rect(piece, (BAR_H * 0.5).min(piece.width() * 0.5), slice.tone.gamma_multiply(dim));
    }
}

fn slices(stats: &Snapshot) -> Vec<Slice> {
    let storage = &stats.storage;
    let unmeasured = if storage.measuring { 0 } else { used(stats).saturating_sub(storage.measured()) };
    let mut system = storage.part(Part::System).map_or(0, |usage| usage.size) + unmeasured;
    let mut slices = Vec::new();
    let plain = storage.parts.iter().filter(|usage| usage.part != Part::System);
    for (index, usage) in plain.enumerate() {
        match SHADES.get(index).filter(|_| slices.len() < PARTS_SHOWN) {
            Some(shade) => {
                slices.push(Slice {
                    part: usage.part,
                    size: usage.size,
                    tone: color::white(*shade),
                    opens: opens(usage),
                });
            }
            None => system += usage.size,
        }
    }
    if system > 0 {
        let opens = storage.part(Part::System).is_some_and(opens);
        slices.push(Slice { part: Part::System, size: system, tone: color::GRAY.gamma_multiply(0.55), opens });
    }
    slices
}

fn opens(usage: &PartUsage) -> bool {
    !usage.items.is_empty()
}

fn shown_items(usage: &PartUsage) -> &[Item] {
    &usage.items[..usage.items.len().min(ITEMS_SHOWN)]
}

fn rest_of(usage: &PartUsage) -> u64 {
    usage.size.saturating_sub(shown_items(usage).iter().map(|item| item.size).sum())
}

fn icon_center(row: Rect) -> Pos2 {
    pos2(row.left() + ICON * 0.5, row.center().y)
}

fn name_at(row: Rect) -> Pos2 {
    pos2(row.left() + ICON + ICON_GAP, row.center().y)
}

fn value_at(row: Rect) -> Pos2 {
    pos2(row.right() - TRAIL - TRAIL_GAP, row.center().y)
}

fn used(stats: &Snapshot) -> u64 {
    match (stats.disk_free, stats.disk_total) {
        (Some(free), Some(total)) => total.saturating_sub(free),
        _ => 0,
    }
}

fn summary(stats: &Snapshot) -> String {
    match stats.disk_total {
        Some(total) if total > 0 => format!("{} of {}", format_bytes(used(stats)), format_bytes(total)),
        _ => format_bytes(stats.storage.measured()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::Storage;

    const GB: u64 = 1_000_000_000;

    fn usage(part: Part, gigabytes: u64, items: usize) -> Arc<PartUsage> {
        let item = |index: usize| Item {
            name: format!("item {index}"),
            path: format!("/{index}").into(),
            size: GB,
            folder: false,
            icon: None,
        };
        Arc::new(PartUsage { part, size: gigabytes * GB, items: (0..items).map(item).collect() })
    }

    fn stats(parts: Vec<Arc<PartUsage>>, measuring: bool) -> Snapshot {
        Snapshot {
            disk_free: Some(100 * GB),
            disk_total: Some(500 * GB),
            storage: Storage { parts, measuring },
            ..Snapshot::default()
        }
    }

    #[test]
    fn parts_by_size_system_gets_rest() {
        let parts = vec![usage(Part::Documents, 90, 3), usage(Part::Apps, 30, 2), usage(Part::Developer, 20, 1)];
        let stats = stats(parts, false);
        let slices = slices(&stats);
        let order: Vec<Part> = slices.iter().map(|slice| slice.part).collect();
        assert_eq!(order, [Part::Documents, Part::Apps, Part::Developer, Part::System]);
        assert_eq!(slices[3].size, 260 * GB, "400 used minus 140 measured");
        assert!(slices[0].opens && !slices[3].opens);
        assert_eq!(slices[0].tone, color::white(SHADES[0]));
        assert_eq!(summary(&stats), "400 GB of 500 GB");
    }

    #[test]
    fn nothing_guessed_while_scanning() {
        let measuring = stats(vec![usage(Part::Apps, 30, 0)], true);
        assert_eq!(slices(&measuring).iter().map(|slice| slice.part).collect::<Vec<_>>(), [Part::Apps]);
        let many: Vec<_> =
            Part::ALL.into_iter().filter(|part| !matches!(part, Part::System)).map(|part| usage(part, 10, 1)).collect();
        let slices = slices(&stats(many, false));
        assert_eq!(slices.len(), PARTS_SHOWN + 1);
        assert_eq!(slices.last().map(|slice| slice.part), Some(Part::System));
        let listed: u64 = slices.iter().map(|slice| slice.size).sum();
        assert_eq!(listed, 400 * GB, "every used byte is in exactly one slice");
    }

    #[test]
    fn part_lists_seven_items() {
        let apps = usage(Part::Apps, 30, 12);
        assert_eq!(shown_items(&apps).len(), ITEMS_SHOWN);
        assert_eq!(rest_of(&apps), 23 * GB);
        let mut list = StorageList::default();
        list.step_to(Some(Part::Apps));
        assert_eq!((list.chosen, list.from, list.enter.value), (Some(Part::Apps), 1.0, 0.0));
        list.step_to(None);
        assert_eq!((list.chosen, list.from), (None, -1.0));
    }
}
