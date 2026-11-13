//! The panel's header: the tab row with its sliding pill, the gear for settings and the Find button.

use egui::{Id, Painter, Rect, Sense, Vec2, pos2, vec2};

use crate::bridge::{Page, Tab};
use crate::notch::Notch;
use crate::platform::Style;
use crate::ui::canvas::{Canvas, fade};
use crate::ui::color::{self, white};
use crate::ui::element::style::glyph as icon_glyph;
use crate::ui::glass::{self, Glass};
use crate::ui::glyph::{self, Glyph};
use crate::ui::metrics::PAD;
use crate::ui::spring::lerp;
use crate::ui::{controls, rows, scroll, swipe};

mod badge;
mod corners;
mod pill;

pub(in crate::notch) use badge::Badges;
pub(in crate::notch) use pill::LiquidPill;

use corners::paint_button;

const BAR_H: f32 = 30.0;
const RESTING: f32 = 0.42;
const SLOT: f32 = 42.0;
const SLOT_MIN: f32 = 20.0;
const ICON: f32 = 14.0;
const PILL_INSET: f32 = 1.0;
const SEATED_BELOW: f32 = 0.05;
const HOVER_TINT: f32 = 0.7;
const GEAR: f32 = 28.0;
const GEAR_GLYPH: f32 = 15.0;
const GEAR_GAP: f32 = 6.0;
const BUDDING: f32 = 0.6;

pub(in crate::notch) struct Header {
    pub(in crate::notch) bar: Rect,
    pub(in crate::notch) gear: Rect,
    pub(in crate::notch) find: Rect,
    pub(in crate::notch) body_top: f32,
    lane: Lane,
}

#[derive(Clone, Copy)]
struct Lane {
    left: f32,
    right: f32,
    anchor: f32,
    center_y: f32,
}

impl Notch {
    pub(in crate::notch) fn header_height(&self) -> f32 {
        match self.style {
            Style::Notch => self.notch.y + 44.0,
            Style::Island => 48.0,
        }
    }

    pub(in crate::notch) fn header(&self) -> Header {
        let panel = self.body_rect(self.expanded_geometry());
        let body_top = panel.top() + self.header_height();
        let (center_y, gear_y) = match self.style {
            Style::Notch => (panel.top() + self.notch.y + 19.0, panel.top() + self.notch.y * 0.5),
            Style::Island => (panel.top() + 23.0, panel.top() + 23.0),
        };
        let gear = Rect::from_center_size(pos2(panel.right() - PAD - GEAR * 0.5, gear_y), Vec2::splat(GEAR));
        let find = Rect::from_center_size(pos2(panel.left() + PAD + GEAR * 0.5, gear_y), Vec2::splat(GEAR));
        let (left, right) = match self.style {
            Style::Notch => (panel.left() + PAD, panel.right() - PAD),
            Style::Island => (find.right() + GEAR_GAP, gear.left() - GEAR_GAP),
        };
        let lane = Lane { left, right, anchor: panel.center().x, center_y };
        Header { bar: row(lane, self.tabs.len() as f32), gear, find, body_top, lane }
    }

    pub(super) fn header_ui(&mut self, ui: &egui::Ui, painter: &Painter, interactive: bool, badges: Badges) -> Canvas {
        let header = self.header();
        let mut canvas = Canvas::new(painter);
        let (settings, finding, emphasis) = (self.settings_shown(), self.shown(Page::Find), self.tab_emphasis());
        let to = self.pager.tab.target;
        let from = self.pager.tab_index(self.pager.from).unwrap_or(to);
        let sweep = Sweep { from, to, value: self.pager.tab.value, emphasis };
        let slots = layout(header.bar, self.tabs.len());
        let mut touch = Touch::default();
        let (mut gear_hovered, mut find_hovered) = (false, false);
        if interactive {
            touch = self.tabs_on_touch(ui, &slots);
            gear_hovered = self.settings_on_click(ui, header.gear);
            find_hovered = self.find_on_click(ui, header.find);
        }
        self.pill_toward(&slots, touch.held_at);
        let hover: Vec<f32> = slots
            .iter()
            .enumerate()
            .map(|(i, slot)| {
                let on = touch.hover_x.is_some_and(|x| slot.x_range().contains(x));
                controls::animate(ui, Id::new(("notchbase.page.hover", i)), on)
            })
            .collect();
        let pill = self.tab_pill.rect(header.bar.center().y, header.bar.height() - 2.0 * PILL_INSET);
        let shown = emphasis.max(self.tab_pill.lift().min(1.0));
        let tabs = self.gliding_tabs(header.lane);
        let bar = TabBar { tabs: &tabs, hover: &hover, badges };
        paint_pill(&mut canvas, pill, self.tab_pill.glass(), shown);
        paint_tabs(&mut canvas, &bar, Lens { pill, lift: self.tab_pill.lift() }, sweep);
        let gear_hover = controls::animate(ui, Id::new("notchbase.settings.hover"), gear_hovered);
        paint_button(&mut canvas, header.gear, Glyph::Gear, GEAR_GLYPH, settings, gear_hover);
        badge::paint(&mut canvas, badge::at(header.gear.center(), GEAR_GLYPH), badges.gear());
        let find_hover = controls::animate(ui, Id::new("notchbase.find.hover"), find_hovered);
        paint_button(&mut canvas, header.find, Glyph::Search, GEAR_GLYPH, finding, find_hover);
        canvas
    }

    fn gliding_tabs(&mut self, lane: Lane) -> Vec<Gliding> {
        let seen = self.panel_alpha > SEATED_BELOW && !self.tab_rows.is_empty() && self.preview.is_none();
        self.tab_rows.follow(self.tabs.iter().map(|tab| (tab.key.clone(), tab.clone())), seen);
        let total: f32 = self.tab_rows.iter().map(|(_, _, shown)| shown).sum();
        let bar = row(lane, total);
        let slot = bar.width() / total.max(f32::EPSILON);
        let mut left = bar.left();
        self.tab_rows
            .iter()
            .map(|(key, tab, shown)| {
                let rect = Rect::from_min_size(pos2(left, bar.top()), vec2(slot * shown, bar.height()));
                left += slot * shown;
                let index = self.tabs.iter().position(|real| real.key == *key);
                let seat = Rect::from_center_size(rect.center(), vec2(slot, bar.height()));
                Gliding { tab: tab.clone(), seat, shown, index }
            })
            .collect()
    }

    fn tabs_on_touch(&mut self, ui: &egui::Ui, slots: &[Rect]) -> Touch {
        let (Some(first), Some(last)) = (slots.first(), slots.last()) else { return Touch::default() };
        let strip = Rect::from_x_y_ranges(first.left()..=last.right(), first.y_range());
        let response = ui.interact(strip, Id::new("notchbase.tabs"), Sense::click_and_drag());
        let pointer = response.interact_pointer_pos().or(response.hover_pos()).map(|at| at.x);
        self.tab_pill.hold(response.is_pointer_button_down_on());
        if (response.clicked() || response.drag_stopped())
            && let Some(page) = pointer.and_then(|x| self.tabs.get(nearest(slots, x))).map(|tab| tab.page)
        {
            self.switch_to(page);
        }
        let held_at = if response.dragged() { pointer } else { None };
        Touch { hover_x: response.hover_pos().map(|at| at.x), held_at }
    }

    pub(super) fn swipe_tabs(&mut self, ui: &egui::Ui, area: Rect) {
        if !ui.rect_contains_pointer(area) || scroll::claimed(ui) {
            return;
        }
        let wheel = scroll::read(ui);
        let across = if wheel.smooth { scroll::take(ui, wheel.across()) } else { 0.0 };
        if across == 0.0 {
            return;
        }
        let step = swipe::once(ui, Id::new("notchbase.tabs"), across);
        let Some(at) = self.pager.tab_index(self.pager.page).filter(|_| step != 0) else { return };
        let next = usize::try_from(at as i64 + i64::from(step)).ok().and_then(|next| self.tabs.get(next));
        if let Some(page) = next.map(|tab| tab.page) {
            self.switch_to(page);
        }
    }

    fn pill_toward(&mut self, slots: &[Rect], held_at: Option<f32>) {
        let (Some(first), Some(last)) = (slots.first(), slots.last()) else { return };
        let chosen = (self.pager.tab.target.round().max(0.0) as usize).min(slots.len() - 1);
        let (left, right) = match held_at {
            Some(x) => {
                let width = slots[nearest(slots, x)].width() - 2.0 * PILL_INSET;
                let center = x.clamp(first.center().x, last.center().x);
                (center - width * 0.5, center + width * 0.5)
            }
            None => {
                let seat = slots[chosen].shrink(PILL_INSET);
                (seat.left(), seat.right())
            }
        };
        if self.panel_alpha < SEATED_BELOW {
            self.tab_pill.seat(left, right);
        } else {
            self.tab_pill.follow(left, right);
        }
    }

    fn settings_shown(&self) -> f32 {
        (self.shown(Page::Settings) + self.shown(Page::Notifications)).min(1.0)
    }

    pub(in crate::notch) fn tab_emphasis(&self) -> f32 {
        let aside = self.shown(Page::Find) + self.shown(Page::WhatsNew) + self.shown(Page::Welcome);
        1.0 - (self.settings_shown() + aside).min(1.0)
    }

    fn shown(&self, page: Page) -> f32 {
        let progress = self.pager.progress();
        match (self.pager.from == page, self.pager.page == page) {
            (_, true) if self.pager.from == self.pager.page => 1.0,
            (false, true) => progress,
            (true, false) => 1.0 - progress,
            _ => 0.0,
        }
    }
}

fn row(lane: Lane, tabs: f32) -> Rect {
    let count = tabs.max(1.0);
    let slot = ((lane.right - lane.left) / count).clamp(SLOT_MIN, SLOT);
    let width = slot * tabs.max(0.0);
    let start = (lane.anchor - width * 0.5).min(lane.right - width).max(lane.left);
    Rect::from_min_size(pos2(start, lane.center_y - BAR_H * 0.5), vec2(width, BAR_H))
}

#[derive(Clone, Copy)]
struct Sweep {
    from: f32,
    to: f32,
    value: f32,
    emphasis: f32,
}

impl Sweep {
    fn open(&self, index: usize) -> f32 {
        let span = self.to - self.from;
        let arrived = if span.abs() < 0.5 { 1.0 } else { ((self.value - self.from) / span).clamp(0.0, 1.0) };
        let at = index as f32;
        let open = if (at - self.to).abs() < 0.5 {
            arrived
        } else if (at - self.from).abs() < 0.5 {
            1.0 - arrived
        } else {
            0.0
        };
        open * self.emphasis
    }
}

fn layout(bar: Rect, count: usize) -> Vec<Rect> {
    let width = bar.width() / count.max(1) as f32;
    (0..count)
        .map(|i| Rect::from_min_size(pos2(bar.left() + i as f32 * width, bar.top()), vec2(width, bar.height())))
        .collect()
}

fn nearest(slots: &[Rect], x: f32) -> usize {
    slots
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| (a.center().x - x).abs().total_cmp(&(b.center().x - x).abs()))
        .map_or(0, |(index, _)| index)
}

#[derive(Default)]
struct Touch {
    hover_x: Option<f32>,
    held_at: Option<f32>,
}

struct Lens {
    pill: Rect,
    lift: f32,
}

struct Gliding {
    tab: Tab,
    seat: Rect,
    shown: f32,
    index: Option<usize>,
}

struct TabBar<'a> {
    tabs: &'a [Gliding],
    hover: &'a [f32],
    badges: Badges,
}

fn paint_pill(canvas: &mut Canvas, pill: Rect, look: Glass, shown: f32) {
    if shown <= 0.001 {
        return;
    }
    let mark = canvas.mark();
    glass::paint(canvas, pill, pill.height() * 0.5, look);
    for shape in canvas.since(mark) {
        fade(shape, shown);
    }
}

fn paint_tabs(canvas: &mut Canvas, bar: &TabBar, lens: Lens, sweep: Sweep) {
    for gliding in bar.tabs {
        let (tab, at) = (&gliding.tab, gliding.seat.center());
        let covered = gliding.index.map_or(0.0, |_| pill::coverage(lens.pill, gliding.seat));
        let hover = gliding.index.and_then(|i| bar.hover.get(i)).copied().unwrap_or(0.0);
        let open = gliding.index.map_or(0.0, |i| sweep.open(i));
        let lit = open.max(covered * lens.lift.min(1.0)).max(HOVER_TINT * hover);
        let tint = color::mix(white(RESTING), white(1.0), lit).gamma_multiply(rows::alpha(gliding.shown));
        let size = ICON * pill::magnify(lens.lift, covered) * lerp(BUDDING, 1.0, gliding.shown);
        glyph::paint(canvas, icon_glyph(tab.icon), at, size, tint);
        if gliding.shown < 0.99 {
            continue;
        }
        badge::paint(canvas, badge::beside(at, size), bar.badges.tab(tab.page));
    }
}

#[cfg(test)]
mod tests;
