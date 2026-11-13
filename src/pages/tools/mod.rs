//! The Tools page: a row of tool icons, a card for the one you linger on, and a slim row for each tool that runs.

mod capture;
mod focus;
mod launcher;
mod reorder;
mod running;
mod shortcut;
mod tile;
pub(in crate::pages) mod well;

use std::collections::HashMap;

use egui::{Align2, Rect, pos2, vec2};
use notchbase_sdk::protocol::{Surface, TileNode, UiValue};

use crate::bridge::{Action, ToolSlot, ToolView};
use crate::ui::canvas::fade;
use crate::ui::color::secondary_label;
use crate::ui::element::{Env, Scene, SurfaceState, Textures};
use crate::ui::metrics::{GAP, SECTION_GAP};
use crate::ui::spring::{POINT_PRECISION, Spring, UNIT_PRECISION, smoothstep};
use crate::ui::{font, motion, scroll};

use super::PageCtx;
use focus::Focus;
use reorder::Reorder;
use tile::{TILE_H, TileState};

pub use capture::KeyCapture;

const EMPTY: &str = "No tools yet";
const EMPTY_GAP: f32 = 4.0;
const ROW_SLOT: f32 = running::ROW_H + GAP;
const CARD_SLOT: f32 = SECTION_GAP + TILE_H;
const EMERGE_MARGIN: f32 = 8.0;
const GONE: f32 = 0.001;
const CARD_GONE_BY: f32 = 0.5;
const CARD_SHOWN_FROM: f32 = 0.25;

struct Showing {
    tile: TileNode,
    appear: Spring,
}

pub struct ToolsPage {
    cells: HashMap<ToolSlot, TileState>,
    rows: HashMap<ToolSlot, TileState>,
    showing: HashMap<ToolSlot, Showing>,
    fresh: bool,
    card: TileState,
    focus: Focus,
    reorder: Reorder,
    scroll: Spring,
    shown: Option<ToolSlot>,
    was: Option<ToolSlot>,
    open: Spring,
    swap: Spring,
    surface: SurfaceState,
}

impl Default for ToolsPage {
    fn default() -> Self {
        Self {
            cells: HashMap::new(),
            rows: HashMap::new(),
            showing: HashMap::new(),
            fresh: true,
            card: TileState::default(),
            focus: Focus::default(),
            reorder: Reorder::default(),
            scroll: Spring::new(0.0, motion::SMOOTH, POINT_PRECISION),
            shown: None,
            was: None,
            open: Spring::new(0.0, motion::SMOOTH, UNIT_PRECISION),
            swap: Spring::new(1.0, motion::HOVER, UNIT_PRECISION),
            surface: SurfaceState::default(),
        }
    }
}

pub struct Frame<'a> {
    pub env: &'a Env<'a>,
    pub textures: &'a mut Textures,
    pub keys: &'a mut Vec<Rect>,
}

impl ToolsPage {
    pub fn estimate() -> f32 {
        launcher::CELL_H
    }

    pub fn height(&self, tools: &[ToolView]) -> f32 {
        let tiled = tiled(tools);
        let strip: f32 = match self.fresh {
            true => tiled.iter().filter(|(_, tile)| running::is_running(tile)).count() as f32 * ROW_SLOT,
            false => tiled
                .iter()
                .filter_map(|(tool, _)| self.showing.get(&tool.slot))
                .map(|row| appeared(row) * ROW_SLOT)
                .sum(),
        };
        let card = CARD_SLOT * self.open.value.clamp(0.0, 1.0);
        strip + launcher::CELL_H + card + self.surface.drawer.extra()
    }

    pub fn is_animating(&self) -> bool {
        self.surface.is_animating()
            || self.focus.waiting()
            || self.reorder.is_animating()
            || !self.scroll.is_settled()
            || !self.open.is_settled()
            || !self.swap.is_settled()
            || self.showing.values().any(|row| !row.appear.is_settled())
            || self.springs().any(|spring| !spring.is_settled())
    }

    pub fn step(&mut self, dt: f32) {
        self.surface.step(dt);
        self.reorder.step(dt);
        self.scroll.step(dt);
        self.open.step(dt);
        self.swap.step(dt);
        for row in self.showing.values_mut() {
            row.appear.step(dt);
        }
        self.showing.retain(|_, row| row.appear.target > 0.0 || row.appear.value > GONE);
        for spring in self.springs_mut() {
            spring.step(dt);
        }
    }

    pub fn snap(&mut self) {
        self.surface.snap();
        self.reorder.snap();
        self.scroll.snap_to(self.scroll.target);
        self.open.snap_to(self.open.target);
        self.swap.snap_to(self.swap.target);
        self.showing.retain(|_, row| row.appear.target > 0.0);
        self.showing.values_mut().for_each(|row| row.appear.snap_to(1.0));
        for spring in self.springs_mut() {
            spring.snap_to(spring.target);
        }
    }

    pub fn rest(&mut self, actions: &mut Vec<Action>) {
        for state in self.states_mut() {
            state.follow(false, false);
        }
        self.focus.clear();
        self.reorder.rest();
        self.scroll.snap_to(0.0);
        self.shown = None;
        self.open.snap_to(0.0);
        self.showing.clear();
        self.fresh = true;
        self.surface.rest(actions);
    }

    pub fn let_go(&mut self) {
        self.surface.focus = None;
    }

    pub fn ui(&mut self, ctx: &mut PageCtx<'_>, tools: &[ToolView], mut frame: Frame<'_>) -> Vec<Action> {
        let live = ctx.alpha > 0.5;
        let mut actions = Vec::new();
        let tiled = tiled(tools);
        self.cells.retain(|slot, _| tools.iter().any(|tool| tool.slot == *slot));
        self.rows.retain(|slot, _| tools.iter().any(|tool| tool.slot == *slot));
        self.surface.begin();
        let (left, width) = (ctx.rect.left(), ctx.rect.width());
        let mut y = ctx.rect.top();
        self.follow_running(&tiled);
        for (tool, tile) in &tiled {
            let Some(row) = self.showing.get(&tool.slot) else { continue };
            let (appear, kept) = (appeared(row), row.tile.clone());
            let on = running::is_running(tile);
            let tile = if on { *tile } else { &kept };
            let rect = Rect::from_min_size(pos2(left, y), vec2(width, running::ROW_H));
            let mut scene = scene(ctx, &mut frame, &mut self.surface, &mut actions, tool.slot, live && on);
            let mark = scene.canvas.mark();
            let state = self.rows.entry(tool.slot).or_default();
            if running::paint(&mut scene, tile, rect, state, on && pressable(tool, tile)) {
                press(&mut scene, tool, tile);
            }
            emerge(scene.canvas, mark, rect, ROW_SLOT, appear);
            y += ROW_SLOT * appear;
        }
        let (mut hovered, mut clicked) = (None, None);
        let listed: Vec<ToolSlot> = tiled.iter().map(|(tool, _)| tool.slot).collect();
        let row = Rect::from_min_size(pos2(left, y), vec2(width, launcher::CELL_H));
        let strip = self.scrolled(ctx, launcher::Strip { rect: row, count: listed.len(), scroll: 0.0 }, live);
        let order = self.reorder.arrange(&listed);
        let raised = self.reorder.raised();
        let below = order.iter().enumerate().filter(|(_, slot)| Some(**slot) != raised);
        let above = order.iter().enumerate().filter(|(_, slot)| Some(**slot) == raised);
        let mark = ctx.canvas.mark();
        if listed.is_empty() {
            paint_empty(ctx, row);
        }
        let mut edged = false;
        for (index, slot) in below.chain(above) {
            let Some((tool, tile)) = tiled.iter().find(|(tool, _)| tool.slot == *slot) else { continue };
            let home = strip.home(index);
            let placed = strip.shown(Rect::from_min_size(self.reorder.place(tool.slot, home.min), home.size()));
            let lifted = raised == Some(tool.slot);
            let rect = placed;
            let surfaced = if lifted { 1.0 } else { strip.surfaced(rect) };
            if lifted && !std::mem::replace(&mut edged, true) {
                edge(ctx, mark, &strip);
            }
            if !lifted && !rect.intersects(strip.window()) {
                continue;
            }
            let mut scene = scene(ctx, &mut frame, &mut self.surface, &mut actions, tool.slot, live);
            let state = self.cells.entry(tool.slot).or_default();
            let cell = launcher::Cell {
                focused: self.shown == Some(tool.slot),
                pressable: pressable(tool, tile),
                movable: listed.len() > 1,
                lift: lifted.then(|| self.reorder.scale(tool.slot, state.pressed())),
                surfaced,
            };
            let touch = launcher::paint(&mut scene, tile, rect, state, cell);
            hovered = hovered.or(touch.hovered.then_some(tool.slot));
            if touch.clicked {
                clicked = Some(tool.slot);
                if pressable(tool, tile) {
                    press(&mut scene, tool, tile);
                }
            }
            if let Some(pointer) = touch.grabbed {
                self.reorder.grab(tool.slot, home.min, strip.along(pointer), &listed);
            }
            if let Some(pointer) = touch.dragged {
                self.reorder.drag(strip.along(pointer), strip.index_at(strip.along(pointer)));
                let pulled = self.scroll.target + strip.nudge(pointer, ctx.dt);
                self.scroll.snap_to(pulled.clamp(0.0, strip.reach()));
            }
            if touch.dropped
                && let Some((moved, before)) = self.reorder.drop(&listed)
            {
                actions.push(Action::MoveTool(moved, before));
            }
        }
        if !edged {
            edge(ctx, mark, &strip);
        }
        if self.reorder.holding().is_some() {
            hovered = None;
        }
        y += launcher::CELL_H + SECTION_GAP;
        let card = Rect::from_min_size(pos2(left, y), vec2(width, TILE_H));
        let rebinding = frame.env.shortcuts.iter().find(|view| view.recording).map(|view| view.which.tool);
        let on_card = self.shown.is_some() && ctx.ui.rect_contains_pointer(card);
        let moving = self.reorder.holding().is_some();
        let locked = rebinding.is_some() || self.surface.drawer.extra() > 0.0 || on_card || moving;
        self.focus.follow(hovered, ctx.dt, locked);
        if let Some(slot) = clicked {
            self.focus.switch(slot);
        }
        let slots: Vec<ToolSlot> = tiled.iter().map(|(tool, _)| tool.slot).collect();
        let focused = self.focus.resolve(&slots, rebinding);
        if focused != self.shown {
            self.was = self.shown.filter(|_| focused.is_some());
            self.shown = focused;
            self.swap.snap_to(0.0);
            self.swap.animate_to(1.0, motion::HOVER);
        }
        self.open.animate_to(if focused.is_some() { 1.0 } else { 0.0 }, motion::SMOOTH);
        if let Some((tool, tile)) = tiled.iter().find(|(tool, _)| Some(tool.slot) == focused) {
            let open = self.open.value.clamp(0.0, 1.0);
            let swap = self.swap.value.clamp(0.0, 1.0);
            let leaving =
                self.was.and_then(|slot| tiled.iter().find(|(tool, _)| tool.slot == slot)).filter(|_| swap < 1.0);
            let start = ctx.canvas.mark();
            if let Some((old, old_tile)) = leaving {
                let mut unheard = Vec::new();
                let mut scene = scene(ctx, &mut frame, &mut self.surface, &mut unheard, old.slot, false);
                let contents = tile::paint(&mut scene, old_tile, card, &mut TileState::default(), (false, false));
                for shape in scene.canvas.since(contents) {
                    fade(shape, 1.0 - smoothstep(0.0, CARD_GONE_BY, swap));
                }
            }
            let mut scene = scene(ctx, &mut frame, &mut self.surface, &mut actions, tool.slot, live);
            let mark = start;
            let contents = tile::paint(&mut scene, tile, card, &mut self.card, (tool.page.is_some(), true));
            let incoming = if leaving.is_some() { smoothstep(CARD_SHOWN_FROM, 1.0, swap) } else { swap };
            for shape in scene.canvas.since(contents) {
                fade(shape, incoming);
            }
            let slot = Rect::from_min_size(pos2(left, card.top() - SECTION_GAP), vec2(width, CARD_SLOT));
            emerge(scene.canvas, mark, slot, CARD_SLOT, open);
            if scene.state.drawer.anchored() {
                let mut drawer = std::mem::take(&mut scene.state.drawer);
                drawer.paint(&mut scene, card.bottom() - (1.0 - open) * CARD_SLOT, left, width);
                scene.state.drawer = drawer;
            }
        }
        self.surface.end(ctx.ui, &mut actions);
        actions
    }

    fn follow_running(&mut self, tiled: &[(&ToolView, &TileNode)]) {
        let fresh = std::mem::take(&mut self.fresh);
        self.showing.retain(|slot, _| tiled.iter().any(|(tool, _)| tool.slot == *slot));
        for (tool, tile) in tiled {
            let on = running::is_running(tile);
            match self.showing.get_mut(&tool.slot) {
                Some(row) => {
                    if on {
                        row.tile = (*tile).clone();
                    }
                    row.appear.animate_to(if on { 1.0 } else { 0.0 }, motion::SMOOTH);
                }
                None if on => {
                    let mut appear = Spring::new(if fresh { 1.0 } else { 0.0 }, motion::SMOOTH, UNIT_PRECISION);
                    appear.animate_to(1.0, motion::SMOOTH);
                    self.showing.insert(tool.slot, Showing { tile: (*tile).clone(), appear });
                }
                None => {}
            }
        }
    }

    fn scrolled(&mut self, ctx: &PageCtx<'_>, strip: launcher::Strip, live: bool) -> launcher::Strip {
        let reach = strip.reach();
        if live && strip.scrolls() && ctx.ui.rect_contains_pointer(strip.rect) {
            let wheel = scroll::read(ctx.ui);
            let across = scroll::take(ctx.ui, wheel.across());
            match (across, wheel.smooth) {
                (0.0, _) => {}
                (across, true) => {
                    let to = self.scroll.target - across * scroll::POINTS_PER_NOTCH;
                    self.scroll.snap_to(to.clamp(0.0, reach));
                }
                (across, false) => {
                    let to = self.scroll.target - across * strip.cell_width();
                    self.scroll.animate_to(to.clamp(0.0, reach), motion::SMOOTH);
                }
            }
        }
        if self.scroll.target > reach {
            self.scroll.snap_to(reach);
        }
        launcher::Strip { scroll: self.scroll.value.clamp(0.0, reach), ..strip }
    }

    #[cfg(test)]
    pub fn scroll(&self) -> f32 {
        self.scroll.value
    }

    #[cfg(test)]
    pub fn scroll_to(&mut self, scroll: f32) {
        self.scroll.snap_to(scroll);
    }

    #[cfg(test)]
    pub fn focused(&self) -> Option<ToolSlot> {
        self.shown
    }

    fn springs(&self) -> impl Iterator<Item = &Spring> {
        let states = self.cells.values().chain(self.rows.values()).chain([&self.card]);
        states.flat_map(TileState::springs)
    }

    fn springs_mut(&mut self) -> impl Iterator<Item = &mut Spring> {
        self.states_mut().flat_map(TileState::springs_mut)
    }

    fn states_mut(&mut self) -> impl Iterator<Item = &mut TileState> {
        self.cells.values_mut().chain(self.rows.values_mut()).chain([&mut self.card])
    }
}

#[cfg(test)]
pub fn running_slot() -> f32 {
    ROW_SLOT
}

fn appeared(row: &Showing) -> f32 {
    row.appear.value.clamp(0.0, 1.0)
}

fn emerge(canvas: &mut crate::ui::canvas::Canvas, mark: crate::ui::canvas::Mark, slot: Rect, reach: f32, shown: f32) {
    if shown >= 1.0 {
        return;
    }
    let hidden = vec2(0.0, -(1.0 - shown) * reach);
    for shape in canvas.since(mark) {
        shape.translate(hidden);
        fade(shape, shown);
    }
    let band = Rect::from_min_max(
        slot.min - vec2(EMERGE_MARGIN, 0.0),
        slot.min + vec2(slot.width() + EMERGE_MARGIN, reach * shown),
    );
    canvas.clip(mark, band);
}

fn edge(ctx: &mut PageCtx<'_>, mark: crate::ui::canvas::Mark, strip: &launcher::Strip) {
    if strip.scrolls() {
        ctx.canvas.clip(mark, strip.window());
    }
}

fn tiled(tools: &[ToolView]) -> Vec<(&ToolView, &TileNode)> {
    tools.iter().filter_map(|tool| Some((tool, tool.tile.as_ref()?))).collect()
}

fn pressable(tool: &ToolView, tile: &TileNode) -> bool {
    tile.enabled && (tile.on_press.is_some() || tool.page.is_some())
}

fn press(scene: &mut Scene, tool: &ToolView, tile: &TileNode) {
    match tile.on_press {
        Some(handler) => scene.emit(Some(handler), UiValue::Press),
        None => scene.actions.push(Action::OpenToolPage(tool.slot)),
    }
}

fn scene<'s>(
    ctx: &'s mut PageCtx<'_>,
    frame: &'s mut Frame<'_>,
    state: &'s mut SurfaceState,
    actions: &'s mut Vec<Action>,
    slot: ToolSlot,
    live: bool,
) -> Scene<'s> {
    Scene {
        ui: ctx.ui,
        canvas: ctx.canvas,
        env: frame.env,
        state,
        textures: frame.textures,
        actions,
        keys: frame.keys,
        slot,
        surface: Surface::Tile,
        live,
    }
}

fn paint_empty(ctx: &mut PageCtx<'_>, row: Rect) {
    let at = pos2(row.left() + EMPTY_GAP, launcher::well_center(row).y);
    ctx.canvas.text(at, Align2::LEFT_CENTER, EMPTY, font::callout(), secondary_label());
}

#[cfg(test)]
mod tests {
    use notchbase_sdk::protocol::{Node, TileNode, Tone};

    use super::*;

    fn tool(slot: u16, tint: Option<Tone>) -> ToolView {
        let tile = TileNode {
            title: format!("Tool {slot}"),
            icon: Node::Spacer { size: None },
            well: None,
            tint,
            wide: false,
            enabled: true,
            on_press: None,
            shortcut: None,
            accessory: None,
            footer: None,
        };
        ToolView {
            slot: ToolSlot(slot),
            id: format!("tool-{slot}"),
            name: String::new(),
            tile: Some(tile),
            page: None,
            live: None,
            settings: Vec::new(),
            failure: None,
        }
    }

    #[test]
    fn icons_alone_until_card() {
        let mut page = ToolsPage::default();
        assert_eq!(page.height(&[]), launcher::CELL_H, "with no tools the empty row still stands");
        let four: Vec<ToolView> = (1..=4).map(|slot| tool(slot, None)).collect();
        assert_eq!(page.height(&four), ToolsPage::estimate());
        let five: Vec<ToolView> = (1..=5).map(|slot| tool(slot, None)).collect();
        assert_eq!(page.height(&five), launcher::CELL_H, "five still make one row");
        let six: Vec<ToolView> = (1..=6).map(|slot| tool(slot, None)).collect();
        assert_eq!(page.height(&six), launcher::CELL_H, "more than fit scroll sideways instead of adding a row");
        page.open.snap_to(1.0);
        assert_eq!(page.height(&six), launcher::CELL_H + SECTION_GAP + TILE_H);
        page.scroll.snap_to(80.0);
        page.rest(&mut Vec::new());
        assert_eq!(page.height(&six), launcher::CELL_H, "closing the panel puts the card away");
        assert_eq!(page.scroll(), 0.0, "and brings the row back to its start");
    }

    #[test]
    fn running_tools_add_rows() {
        let page = ToolsPage::default();
        let tools = [tool(1, Some(Tone::Red)), tool(2, None), tool(3, Some(Tone::Orange))];
        let quiet = [tool(1, None), tool(2, None), tool(3, None)];
        assert_eq!(page.height(&tools), page.height(&quiet) + 2.0 * (running::ROW_H + GAP));
    }

    #[test]
    fn page_stays_one_row_tall() {
        let mut page = ToolsPage::default();
        let many: Vec<ToolView> = (1..=16).map(|slot| tool(slot, None)).collect();
        assert_eq!(page.height(&many), launcher::CELL_H);
        page.open.snap_to(1.0);
        assert_eq!(page.height(&many), launcher::CELL_H + SECTION_GAP + TILE_H);
    }
}
