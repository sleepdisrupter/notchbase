//! The apps behind a gauge, busiest first, each with a way to quit it.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::epaint::RectShape;
use egui::{
    Align2, ColorImage, Id, ImageData, Pos2, Rect, Sense, TextureFilter, TextureHandle, TextureOptions, pos2, vec2,
};

use super::format::{format_bytes, share_label};
use crate::bridge::Action;
use crate::pages::PageCtx;
use crate::stats::{AppUsage, LISTED, Resource, Snapshot, top};
use crate::ui::canvas::{Canvas, fade};
use crate::ui::glyph::{self, Glyph};
use crate::ui::spring::{POINT_PRECISION, Spring, UNIT_PRECISION};
use crate::ui::{color, controls, font, metrics, motion};

const ROW_H: f32 = 28.0;
const ROWS: usize = LISTED + 1;
const LIST_H: f32 = ROW_H * ROWS as f32;
const ROW_BLEED: f32 = 6.0;
const ICON: f32 = 20.0;
const ICON_RADIUS: f32 = 5.0;
const ICON_GAP: f32 = 10.0;
const CLOSE: f32 = 18.0;
const CLOSE_GLYPH: f32 = 15.0;
const CLOSE_GAP: f32 = 6.0;
const NAME_GAP: f32 = 12.0;
const REST_GLYPH: f32 = 13.0;
const INTERACTIVE_FROM: f32 = 0.5;
const GIVE_UP_AFTER: Duration = Duration::from_secs(3);
const REST: &str = "Everything else";
const QUITTING: &str = "Quitting…";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Quit {
    Offered,
    Waiting,
    Stuck,
}

impl Quit {
    fn of(asked: Option<Instant>, now: Instant) -> Self {
        match asked {
            None => Self::Offered,
            Some(at) if now.saturating_duration_since(at) < GIVE_UP_AFTER => Self::Waiting,
            Some(_) => Self::Stuck,
        }
    }

    fn forces(self, option_held: bool) -> bool {
        option_held || self == Self::Stuck
    }
}

struct Icon {
    source: Arc<ColorImage>,
    texture: TextureHandle,
}

struct Placed {
    y: Spring,
    shown: Spring,
}

impl Placed {
    fn at(rank: usize, shown: f32) -> Self {
        let mut placed = Self {
            y: Spring::new(rank as f32 * ROW_H, motion::SMOOTH, POINT_PRECISION),
            shown: Spring::new(shown, motion::SMOOTH, UNIT_PRECISION),
        };
        placed.shown.animate_to(1.0, motion::SMOOTH);
        placed
    }

    fn is_settled(&self) -> bool {
        self.y.is_settled() && self.shown.is_settled()
    }
}

pub(super) struct AppList {
    open: Spring,
    by: Resource,
    shown: Vec<String>,
    placed: HashMap<String, Placed>,
    asked: HashMap<String, Instant>,
    icons: HashMap<String, Icon>,
}

impl Default for AppList {
    fn default() -> Self {
        Self {
            open: Spring::new(0.0, motion::SMOOTH, UNIT_PRECISION),
            by: Resource::Cpu,
            shown: Vec::new(),
            placed: HashMap::new(),
            asked: HashMap::new(),
            icons: HashMap::new(),
        }
    }
}

impl AppList {
    pub(super) fn height(&self) -> f32 {
        self.open.value.clamp(0.0, 1.0) * (metrics::SECTION_GAP + LIST_H)
    }

    pub(super) fn is_animating(&self) -> bool {
        !self.open.is_settled() || self.placed.values().any(|placed| !placed.is_settled())
    }

    pub(super) fn ui(&mut self, ctx: &mut PageCtx<'_>, stats: &Snapshot, top_left: Pos2, width: f32) -> Vec<Action> {
        let listing = stats.listing.filter(|by| by.lists_apps());
        if !self.open.is_settled() || listing.is_some() != (self.open.target > 0.5) {
            self.open.animate_to(if listing.is_some() { 1.0 } else { 0.0 }, motion::SMOOTH);
            self.open.step(ctx.dt);
        }
        self.by = listing.unwrap_or(self.by);
        let room = self.height() - metrics::SECTION_GAP;
        if room <= 0.0 {
            return Vec::new();
        }
        let list = Rect::from_min_size(top_left + vec2(0.0, metrics::SECTION_GAP), vec2(width, LIST_H));
        let live = listing.is_some() && ctx.alpha >= INTERACTIVE_FROM;
        let frozen = live && ctx.ui.rect_contains_pointer(list.expand2(vec2(ROW_BLEED, 0.0)));
        self.follow(&stats.apps, frozen);
        self.place(live, ctx.dt);

        let mut actions = Vec::new();
        let listed: f64 = self
            .shown
            .iter()
            .filter_map(|key| stats.apps.iter().find(|app| &app.key == key))
            .map(|app| amount(app, self.by))
            .sum();
        let mut drawn: Vec<(String, f32, f32)> = self
            .placed
            .iter()
            .map(|(key, placed)| (key.clone(), placed.y.value, placed.shown.value.clamp(0.0, 1.0)))
            .collect();
        drawn.sort_by(|a, b| a.1.total_cmp(&b.1));
        for (key, y, shown) in drawn {
            let Some(app) = stats.apps.iter().find(|app| app.key == key) else { continue };
            let reveal = revealed_at(room, y) * shown;
            if reveal > 0.0 {
                let mark = ctx.canvas.mark();
                let settled = shown > 0.99 && self.shown.contains(&key);
                actions.extend(self.app_row(ctx, app, row_at(list, y), live && settled));
                ctx.canvas.since(mark).iter_mut().for_each(|shape| fade(shape, reveal));
            }
        }
        let reveal = revealed(room, LISTED);
        if let Some(rest) = rest(stats, self.by, listed).filter(|_| reveal > 0.0 && !self.shown.is_empty()) {
            let mark = ctx.canvas.mark();
            rest_row(ctx.canvas, row(list, LISTED), value_text(self.by, rest));
            ctx.canvas.since(mark).iter_mut().for_each(|shape| fade(shape, reveal));
        }
        actions
    }

    fn place(&mut self, animate: bool, dt: f32) {
        let animate = animate && !self.placed.is_empty();
        for (rank, key) in self.shown.iter().enumerate() {
            match self.placed.get_mut(key) {
                Some(placed) => {
                    placed.y.animate_to(rank as f32 * ROW_H, motion::SMOOTH);
                    placed.shown.animate_to(1.0, motion::SMOOTH);
                }
                None => {
                    self.placed.insert(key.clone(), Placed::at(rank, if animate { 0.0 } else { 1.0 }));
                }
            }
        }
        let shown = &self.shown;
        self.placed.retain(|key, placed| {
            if !shown.contains(key) {
                if !animate {
                    return false;
                }
                placed.shown.animate_to(0.0, motion::SMOOTH);
            }
            placed.y.step(dt);
            placed.shown.step(dt);
            placed.shown.target > 0.0 || !placed.shown.is_settled()
        });
    }

    fn follow(&mut self, apps: &[AppUsage], frozen: bool) {
        if frozen && !self.shown.is_empty() {
            self.shown.retain(|key| apps.iter().any(|app| &app.key == key));
        } else {
            self.shown = top(apps, self.by).into_iter().map(|app| app.key.clone()).collect();
        }
        self.asked.retain(|key, _| apps.iter().any(|app| &app.key == key));
        self.icons.retain(|key, _| apps.iter().any(|app| &app.key == key));
    }

    fn app_row(&mut self, ctx: &mut PageCtx<'_>, app: &AppUsage, row: Rect, live: bool) -> Option<Action> {
        let id = Id::new("notchbase.stats.apps").with(&app.key);
        let quit = Quit::of(self.asked.get(&app.key).copied(), ctx.now);
        let close = Rect::from_center_size(pos2(row.right() - CLOSE * 0.5, row.center().y), vec2(CLOSE, CLOSE));
        let offered = live && app.quittable && quit != Quit::Waiting;
        let button = offered.then(|| ctx.ui.interact(close, id.with("quit"), Sense::click()));
        let over_row = live && ctx.ui.rect_contains_pointer(row.expand2(vec2(ROW_BLEED, 0.0)));
        let hover = controls::animate(ctx.ui, id.with("hover"), over_row);
        let over_close = controls::animate(ctx.ui, id.with("close"), button.as_ref().is_some_and(|b| b.hovered()));
        let forces = quit.forces(ctx.ui.input(|input| input.modifiers.alt));

        let c = &mut *ctx.canvas;
        controls::row_highlight(c, row.expand2(vec2(ROW_BLEED, 0.0)), hover);
        let icon = Rect::from_center_size(pos2(row.left() + ICON * 0.5, row.center().y), vec2(ICON, ICON));
        self.paint_icon(ctx.ui.ctx(), c, icon, app);

        let value = if quit == Quit::Waiting { QUITTING.to_owned() } else { value_text(self.by, amount(app, self.by)) };
        let value_right = close.left() - CLOSE_GAP;
        let value_color = if quit == Quit::Waiting { color::tertiary_label() } else { color::secondary_label() };
        let value_rect =
            c.text(pos2(value_right, row.center().y), Align2::RIGHT_CENTER, value, font::body(), value_color);
        let name_x = icon.right() + ICON_GAP;
        let name = controls::truncate(c, &app.name, font::body(), value_rect.left() - NAME_GAP - name_x);
        c.text(pos2(name_x, row.center().y), Align2::LEFT_CENTER, name, font::body(), color::label());

        let shown = if quit == Quit::Stuck { 1.0 } else { hover };
        if offered && shown > 0.01 {
            let rest = if forces { color::RED } else { color::secondary_label() };
            let lit = if forces { color::RED } else { color::label() };
            if over_close > 0.01 {
                c.circle(close.center(), CLOSE * 0.5, color::white(0.10 * over_close));
            }
            let tint = color::mix(rest, lit, over_close).gamma_multiply(shown);
            glyph::paint(c, Glyph::Xmark, close.center(), CLOSE_GLYPH, tint);
        }

        if !button.is_some_and(|b| b.clicked()) {
            return None;
        }
        self.asked.insert(app.key.clone(), ctx.now);
        Some(Action::QuitApp { key: app.key.clone(), name: app.name.clone(), force: forces })
    }

    fn paint_icon(&mut self, ctx: &egui::Context, c: &mut Canvas, rect: Rect, app: &AppUsage) {
        let Some(source) = &app.icon else {
            c.circle(rect.center(), rect.width() * 0.5, color::white(0.14));
            let initial = app.name.chars().find(|c| c.is_alphanumeric()).map_or('?', |c| c.to_ascii_uppercase());
            c.text(rect.center(), Align2::CENTER_CENTER, initial, font::semibold(10.5), color::white(0.85));
            return;
        };
        if self.icons.get(&app.key).is_none_or(|icon| !Arc::ptr_eq(&icon.source, source)) {
            let options = TextureOptions::LINEAR.with_mipmap_mode(Some(TextureFilter::Linear));
            let name = format!("stats-app-icon-{}", app.key);
            let texture = ctx.load_texture(name, ImageData::Color(Arc::clone(source)), options);
            self.icons.insert(app.key.clone(), Icon { source: Arc::clone(source), texture });
        }
        let uv = Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0));
        let texture = self.icons[&app.key].texture.id();
        c.add(RectShape::filled(rect, ICON_RADIUS, color::white(1.0)).with_texture(texture, uv));
    }
}

fn rest_row(c: &mut Canvas, row: Rect, value: String) {
    let icon = pos2(row.left() + ICON * 0.5, row.center().y);
    glyph::paint(c, Glyph::Layers, icon, REST_GLYPH, color::tertiary_label());
    let name = pos2(row.left() + ICON + ICON_GAP, row.center().y);
    c.text(name, Align2::LEFT_CENTER, REST, font::body(), color::secondary_label());
    let value_right = row.right() - CLOSE - CLOSE_GAP;
    c.text(pos2(value_right, row.center().y), Align2::RIGHT_CENTER, value, font::body(), color::tertiary_label());
}

fn row(list: Rect, index: usize) -> Rect {
    row_at(list, ROW_H * index as f32)
}

fn row_at(list: Rect, y: f32) -> Rect {
    Rect::from_min_size(pos2(list.left(), list.top() + y), vec2(list.width(), ROW_H))
}

fn revealed(room: f32, index: usize) -> f32 {
    revealed_at(room, ROW_H * index as f32)
}

fn revealed_at(room: f32, y: f32) -> f32 {
    ((room - y) / ROW_H).clamp(0.0, 1.0)
}

fn amount(app: &AppUsage, of: Resource) -> f64 {
    match of {
        Resource::Cpu => f64::from(app.cpu),
        Resource::Memory | Resource::Disk => app.memory as f64,
    }
}

fn rest(stats: &Snapshot, of: Resource, listed: f64) -> Option<f64> {
    let whole = match of {
        Resource::Cpu => f64::from(stats.cpu?),
        Resource::Memory | Resource::Disk => stats.mem_used? as f64,
    };
    Some((whole - listed).max(0.0))
}

fn value_text(of: Resource, amount: f64) -> String {
    match of {
        Resource::Cpu => share_label(amount),
        Resource::Memory | Resource::Disk => format_bytes(amount as u64),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(key: &str, cpu: f32, memory: u64) -> AppUsage {
        AppUsage { key: key.to_owned(), name: key.to_owned(), icon: None, cpu, memory, quittable: true }
    }

    #[test]
    fn force_quit_waits_three_seconds() {
        let asked = Instant::now();
        assert_eq!(Quit::of(None, asked), Quit::Offered);
        assert_eq!(Quit::of(Some(asked), asked), Quit::Waiting);
        assert_eq!(Quit::of(Some(asked), asked + GIVE_UP_AFTER - Duration::from_millis(1)), Quit::Waiting);
        assert_eq!(Quit::of(Some(asked), asked + GIVE_UP_AFTER), Quit::Stuck);
        assert!(!Quit::Offered.forces(false));
        assert!(Quit::Offered.forces(true), "option-click forces straight away");
        assert!(Quit::Stuck.forces(false));
    }

    #[test]
    fn order_holds_under_pointer() {
        let mut list = AppList::default();
        let mut apps = vec![app("a", 0.3, 0), app("b", 0.2, 0), app("c", 0.1, 0)];
        list.follow(&apps, false);
        assert_eq!(list.shown, ["a", "b", "c"]);
        apps[2].cpu = 0.9;
        list.follow(&apps, true);
        assert_eq!(list.shown, ["a", "b", "c"], "frozen while hovered");
        apps.remove(1);
        list.asked.insert("b".into(), Instant::now());
        list.follow(&apps, true);
        assert_eq!(list.shown, ["a", "c"], "an app that quit leaves even a frozen list");
        assert!(list.asked.is_empty());
        list.follow(&apps, false);
        assert_eq!(list.shown, ["c", "a"]);
    }

    #[test]
    fn reranked_row_glides() {
        let mut list = AppList::default();
        let mut apps = vec![app("a", 0.3, 0), app("b", 0.2, 0), app("c", 0.1, 0)];
        list.follow(&apps, false);
        list.place(true, 1.0 / 60.0);
        assert_eq!(list.placed["c"].y.value, 2.0 * ROW_H, "the first rows are simply there");
        assert_eq!(list.placed["c"].shown.value, 1.0);
        apps[2].cpu = 0.9;
        apps.push(app("d", 0.05, 0));
        list.follow(&apps, false);
        list.place(true, 1.0 / 60.0);
        let c = list.placed["c"].y.value;
        assert!(c > 0.0 && c < 2.0 * ROW_H, "c moves up over a few frames, not in one: {c}");
        assert!(list.placed["d"].shown.value < 0.2, "a newcomer fades in");
        assert!(list.is_animating());
        for _ in 0..120 {
            list.place(true, 1.0 / 60.0);
        }
        assert_eq!(list.placed["c"].y.value, 0.0);
        apps.retain(|app| app.key != "b");
        list.follow(&apps, false);
        list.place(true, 1.0 / 60.0);
        assert!(list.placed.contains_key("b"), "an app that leaves the list fades where it was");
        for _ in 0..120 {
            list.place(true, 1.0 / 60.0);
        }
        assert!(!list.placed.contains_key("b") && !list.is_animating());
    }

    #[test]
    fn rows_appear_as_the_list_opens() {
        assert_eq!(revealed(0.0, 0), 0.0);
        assert_eq!(revealed(ROW_H * 0.5, 0), 0.5);
        assert_eq!(revealed(ROW_H * 0.5, 1), 0.0);
        assert_eq!(revealed(LIST_H, LISTED), 1.0);
        let mut list = AppList::default();
        assert_eq!(list.height(), 0.0);
        list.open.snap_to(1.0);
        assert_eq!(list.height(), metrics::SECTION_GAP + LIST_H);
    }

    #[test]
    fn rest_is_what_listed_leave() {
        let stats = Snapshot { cpu: Some(0.5), mem_used: Some(8_000_000_000), ..Snapshot::default() };
        assert_eq!(rest(&stats, Resource::Cpu, 0.25), Some(0.25));
        assert_eq!(rest(&stats, Resource::Cpu, 0.75), Some(0.0));
        assert_eq!(rest(&stats, Resource::Memory, 3e9), Some(5e9));
        assert_eq!(rest(&Snapshot::default(), Resource::Memory, 0.0), None);
        assert_eq!(value_text(Resource::Cpu, 0.314), "31%");
        assert_eq!(value_text(Resource::Cpu, 0.042), "4.2%");
    }
}
