//! The Shelf page: files dropped on the notch wait here as tiles, ready to drag out, convert or AirDrop.

#[cfg(test)]
mod airdrop_flow;
mod busy;
mod buttons;
mod crossfade;
mod effects;
mod icons;
mod liftoff;
mod status;
mod tiles;
mod work;

use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};

use egui::{Align2, Rect, TextureHandle, TextureOptions, pos2, vec2};

use crate::bridge::Action;
use crate::shelf::{Busy, Item, ItemId, Notice, Shelf};
use crate::ui::canvas::{Canvas, fade};
use crate::ui::color::{label, secondary_label};
use crate::ui::glass::{self, Glass};
use crate::ui::glyph::{self, Glyph};
use crate::ui::motion::Clock;
use crate::ui::rows::Rows;
use crate::ui::spring::{POINT_PRECISION, Spring, SpringConfig, UNIT_PRECISION, smoothstep};
use crate::ui::{font, metrics};

use super::PageCtx;
use buttons::Row;
use crossfade::Crossfade;
use liftoff::Liftoff;
use status::Status;
use tiles::{SCROLL, TILES_H};
use work::Work;

const STATUS_GAP: f32 = 10.0;
const STATUS_H: f32 = 16.0;
const BUTTONS_GAP: f32 = 8.0;
const BUTTONS_H: f32 = 46.0;
const EMPTY_H: f32 = 76.0;
const EMPTY_FROM: f32 = 0.5;
const INTERACTIVE_FROM: f32 = 0.5;
const BYTE_UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
const WHOLE_FROM: f64 = 9.95;
const STAGED: usize = 3;
const RING: SpringConfig = SpringConfig::new(0.35, 1.0);

pub struct ShelfPage {
    selected: Vec<ItemId>,
    formats_open: bool,
    thumbs: HashMap<ItemId, Thumb>,
    tiles: Rows<ItemId, Item>,
    pops: HashMap<ItemId, Spring>,
    known: HashSet<ItemId>,
    scroll: Spring,
    status: Crossfade<Status>,
    buttons: Crossfade<Row>,
    ring: Spring,
    work: Work,
    clock: Clock,
    liftoff: Liftoff,
    staged_airdrop: bool,
    staged_formats: bool,
}

#[derive(Clone, Copy)]
struct Feed<'a> {
    items: &'a [Item],
    busy: Option<&'a Busy>,
    notice: Option<&'a Notice>,
}

struct Thumb {
    rev: u64,
    texture: TextureHandle,
}

impl Default for ShelfPage {
    fn default() -> Self {
        Self {
            selected: Vec::new(),
            formats_open: false,
            thumbs: HashMap::new(),
            tiles: Rows::default(),
            pops: HashMap::new(),
            known: HashSet::new(),
            scroll: Spring::new(0.0, SCROLL, POINT_PRECISION),
            status: Crossfade::new(),
            buttons: Crossfade::new(),
            ring: Spring::new(0.0, RING, UNIT_PRECISION),
            work: Work::default(),
            clock: Clock::default(),
            liftoff: Liftoff::default(),
            staged_airdrop: false,
            staged_formats: false,
        }
    }
}

impl ShelfPage {
    pub fn height(&self, shelf: &Shelf, _width: f32) -> f32 {
        if shelf.items().is_empty() { EMPTY_H } else { TILES_H + STATUS_GAP + STATUS_H + BUTTONS_GAP + BUTTONS_H }
    }

    pub fn is_animating(&self) -> bool {
        !self.pops.is_empty()
            || self.tiles.is_animating()
            || !self.ring.is_settled()
            || !self.scroll.is_settled()
            || !self.status.is_settled()
            || !self.buttons.is_settled()
            || self.work.is_animating()
            || self.liftoff.is_animating()
    }

    pub fn rest(&mut self) {
        self.liftoff.rest();
    }

    pub fn stage_formats(&mut self) {
        self.staged_formats = true;
    }

    /// A preview's few files picked, ready to AirDrop.
    pub fn stage_airdrop(&mut self) {
        self.staged_airdrop = true;
    }

    pub fn ui(&mut self, ctx: &mut PageCtx<'_>, shelf: &Shelf, notice: Option<&Notice>) -> Vec<Action> {
        let items = shelf.items();
        let busy = shelf.busy();
        let mut actions = Vec::new();
        let seen = interactive(ctx) && !self.tiles.is_empty();
        self.tiles.follow(items.iter().map(|item| (item.id, item.clone())), seen);
        self.tiles.step(ctx.dt);
        self.forget_gone(items);
        self.clock.advance(ctx.dt);
        self.work.update(busy.clone(), notice.is_some_and(|notice| notice.ok), ctx.dt);
        self.follow_ring(busy.as_ref().map(|busy| busy.progress), ctx.dt);
        self.liftoff.step(ctx.dt);

        let width = ctx.rect.width();
        let row = Rect::from_min_size(ctx.rect.min, vec2(width, TILES_H));
        let line = Rect::from_min_size(pos2(ctx.rect.left(), row.bottom() + STATUS_GAP), vec2(width, STATUS_H));
        let buttons = Rect::from_min_size(pos2(ctx.rect.left(), line.bottom() + BUTTONS_GAP), vec2(width, BUTTONS_H));
        let feed = Feed { items, busy: busy.as_ref(), notice };
        if items.is_empty() {
            paint_empty(ctx.canvas, ctx.rect, smoothstep(EMPTY_FROM, 1.0, 1.0 - self.tiles.most_shown()));
            self.tiles_ui(ctx, row, items, false, &mut actions);
            self.status_leaves(ctx, line, &feed);
            self.buttons_leave(ctx, buttons, &feed);
            return actions;
        }
        self.upload_thumbs(ctx.ui.ctx(), items);
        self.tiles_ui(ctx, row, items, interactive(ctx), &mut actions);
        self.apply_stage(items);
        self.status_ui(ctx, line, &feed, &mut actions);
        self.buttons_ui(ctx, buttons, &feed, &mut actions);
        actions
    }

    fn follow_ring(&mut self, progress: Option<f32>, dt: f32) {
        match progress {
            Some(progress) => self.ring.animate_to(progress, RING),
            None => self.ring.snap_to(0.0),
        }
        self.ring.step(dt);
    }

    fn apply_stage(&mut self, items: &[Item]) {
        if std::mem::take(&mut self.staged_formats) {
            self.formats_open = true;
        }
        if std::mem::take(&mut self.staged_airdrop) {
            self.selected = items.iter().take(STAGED).map(|item| item.id).collect();
            self.formats_open = false;
        }
    }

    fn scope<'a>(&self, items: &'a [Item]) -> Vec<&'a Item> {
        let selected: Vec<&Item> = items.iter().filter(|item| self.selected.contains(&item.id)).collect();
        if selected.is_empty() { items.iter().collect() } else { selected }
    }

    fn forget_gone(&mut self, items: &[Item]) {
        let alive = |id: &ItemId| items.iter().any(|item| item.id == *id);
        let leaving: Vec<ItemId> = self.tiles.iter().map(|(id, _, _)| *id).collect();
        self.selected.retain(alive);
        self.thumbs.retain(|id, _| alive(id) || leaving.contains(id));
        self.pops.retain(|id, _| alive(id));
        self.known.retain(alive);
        self.liftoff.forget_gone(alive);
    }

    fn upload_thumbs(&mut self, ctx: &egui::Context, items: &[Item]) {
        for item in items {
            let Some(image) = &item.thumb else { continue };
            match self.thumbs.entry(item.id) {
                Entry::Occupied(entry) if entry.get().rev == item.thumb_rev => {}
                Entry::Occupied(mut entry) => {
                    let thumb = entry.get_mut();
                    thumb.texture.set(image.clone(), TextureOptions::LINEAR);
                    thumb.rev = item.thumb_rev;
                }
                Entry::Vacant(entry) => {
                    let name = format!("shelf-thumb-{}", item.id.0);
                    let texture = ctx.load_texture(name, image.clone(), TextureOptions::LINEAR);
                    entry.insert(Thumb { rev: item.thumb_rev, texture });
                }
            }
        }
    }

    fn toggle(&mut self, id: ItemId) {
        if let Some(index) = self.selected.iter().position(|&selected| selected == id) {
            self.selected.remove(index);
        } else {
            self.selected.push(id);
        }
    }
}

fn paint_empty(canvas: &mut Canvas, well: Rect, shown: f32) {
    if shown <= 0.001 {
        return;
    }
    let mark = canvas.mark();
    glass::paint(canvas, well, metrics::CARD_RADIUS, Glass::TILE);
    let cx = well.center().x;
    glyph::paint(canvas, Glyph::Tray, pos2(cx, well.top() + 26.0), 22.0, secondary_label());
    canvas.text(pos2(cx, well.top() + 54.0), Align2::CENTER_CENTER, "Drop files here", font::headline(), label());
    for shape in canvas.since(mark) {
        fade(shape, shown);
    }
}

fn interactive(ctx: &PageCtx<'_>) -> bool {
    ctx.alpha > INTERACTIVE_FROM
}

fn human_duration(seconds: f64) -> String {
    let total = seconds.max(0.0).round() as u64;
    let (hours, minutes, seconds) = (total / 3600, total / 60 % 60, total % 60);
    if hours > 0 { format!("{hours}:{minutes:02}:{seconds:02}") } else { format!("{minutes}:{seconds:02}") }
}

fn human_bytes(bytes: u64) -> String {
    if bytes < 1000 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64 / 1000.0;
    let mut unit = 0;
    while displayed(value) >= 1000.0 && unit < BYTE_UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    let value = displayed(value);
    if value < WHOLE_FROM {
        format!("{value:.1} {}", BYTE_UNITS[unit])
    } else {
        format!("{value:.0} {}", BYTE_UNITS[unit])
    }
}

fn displayed(value: f64) -> f64 {
    if value < WHOLE_FROM { (value * 10.0).round() / 10.0 } else { value.round() }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::Instant;

    use egui::{Color32, ColorImage};

    use super::*;
    use crate::shelf::{Kind, Origin};

    fn image_item(id: u64, thumb_rev: u64) -> Item {
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
            thumb: Some(Arc::new(ColorImage::filled([2, 2], Color32::WHITE))),
            thumb_rev,
            added: Instant::now(),
            origin: Origin::Dropped,
        }
    }

    #[test]
    fn thumbnails_reuse_and_free() {
        let ctx = egui::Context::default();
        let mut page = ShelfPage::default();
        page.upload_thumbs(&ctx, &[image_item(1, 1), image_item(2, 1)]);
        let (kept, dropped) = (page.thumbs[&ItemId(1)].texture.id(), page.thumbs[&ItemId(2)].texture.id());
        page.upload_thumbs(&ctx, &[image_item(1, 1), image_item(2, 1)]);
        assert_eq!(page.thumbs[&ItemId(1)].texture.id(), kept);

        let remaining = [image_item(1, 2)];
        page.forget_gone(&remaining);
        page.upload_thumbs(&ctx, &remaining);
        assert_eq!(page.thumbs.len(), 1);
        assert_eq!((page.thumbs[&ItemId(1)].rev, page.thumbs[&ItemId(1)].texture.id()), (2, kept));
        let textures = ctx.tex_manager();
        assert!(textures.read().meta(kept).is_some());
        assert!(textures.read().meta(dropped).is_none());
    }

    #[test]
    fn human_bytes_reads_like_finder() {
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(999), "999 B");
        assert_eq!(human_bytes(1_000), "1.0 KB");
        assert_eq!(human_bytes(1_234), "1.2 KB");
        assert_eq!(human_bytes(56_000_000), "56 MB");
        assert_eq!(human_bytes(3_400_000_000), "3.4 GB");
        assert_eq!(human_bytes(u64::MAX), "18446744 TB");
    }

    #[test]
    fn durations_read_like_a_player() {
        assert_eq!(human_duration(0.4), "0:00");
        assert_eq!(human_duration(42.0), "0:42");
        assert_eq!(human_duration(62.6), "1:03");
        assert_eq!(human_duration(3723.0), "1:02:03");
        assert_eq!(human_duration(-5.0), "0:00");
    }

    #[test]
    fn human_bytes_rolls_over() {
        assert_eq!(human_bytes(9_940), "9.9 KB");
        assert_eq!(human_bytes(9_960), "10 KB");
        assert_eq!(human_bytes(999_499), "999 KB");
        assert_eq!(human_bytes(999_500), "1.0 MB");
        assert_eq!(human_bytes(999_999), "1.0 MB");
        assert_eq!(human_bytes(999_999_999), "1.0 GB");
    }
}
