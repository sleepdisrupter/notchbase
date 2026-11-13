//! Pages that tools build with the SDK, laid out from the blocks each tool sends, or a note when one stopped.

use std::collections::HashMap;

use egui::{Align2, Id, Rect, pos2, vec2};
use notchbase_sdk::protocol::{Node, Surface};

use super::PageCtx;
use super::tools::Frame;
use crate::bridge::{Action, ToolSlot, ToolView};
use crate::ui::color::secondary_label;
use crate::ui::element::{Measure, Scene, SurfaceState, surface_id};
use crate::ui::font;
use crate::ui::glass::{self, Glass};
use crate::ui::metrics::{CARD_RADIUS, GAP};

const MISSING_H: f32 = 60.0;
const STOPPED: &str = "This tool stopped working";

#[derive(Default)]
pub struct ToolPages {
    surfaces: HashMap<ToolSlot, SurfaceState>,
}

impl ToolPages {
    pub fn height(&self, tool: Option<&ToolView>, measure: &Measure, width: f32) -> f32 {
        let Some((tool, node)) = tool.and_then(|tool| tool.page.as_ref().map(|node| (tool, node))) else {
            return MISSING_H;
        };
        let extra = self.surfaces.get(&tool.slot).map_or(0.0, |surface| surface.drawer.extra());
        blocks(node).iter().map(|block| measure.size(block, width).y).sum::<f32>() + gaps(node) + extra
    }

    pub fn is_animating(&self) -> bool {
        self.surfaces.values().any(SurfaceState::is_animating)
    }

    pub fn step(&mut self, dt: f32) {
        for surface in self.surfaces.values_mut() {
            surface.step(dt);
        }
    }

    pub fn snap(&mut self) {
        for surface in self.surfaces.values_mut() {
            surface.snap();
        }
    }

    pub fn rest(&mut self, actions: &mut Vec<Action>) {
        for surface in self.surfaces.values_mut() {
            surface.rest(actions);
        }
    }

    pub fn let_go(&mut self) {
        self.surfaces.values_mut().for_each(|surface| surface.focus = None);
    }

    pub fn ui(&mut self, ctx: &mut PageCtx<'_>, tool: Option<&ToolView>, frame: Frame<'_>) -> Vec<Action> {
        let mut actions = Vec::new();
        let Some(tool) = tool else { return actions };
        let Some(node) = &tool.page else {
            paint_missing(ctx, tool.failure.as_deref().map_or(STOPPED, |_| STOPPED));
            return actions;
        };
        let live = ctx.alpha > 0.5;
        let surface = self.surfaces.entry(tool.slot).or_default();
        surface.begin();
        let spacing = spacing(node);
        let base = surface_id(tool.slot, Surface::Page);
        let (left, width) = (ctx.rect.left(), ctx.rect.width());
        let mut y = ctx.rect.top();
        for (index, block) in blocks(node).into_iter().enumerate() {
            let anchored = surface.drawer.anchored();
            let mut scene = Scene {
                ui: ctx.ui,
                canvas: ctx.canvas,
                env: frame.env,
                state: surface,
                textures: frame.textures,
                actions: &mut actions,
                keys: frame.keys,
                slot: tool.slot,
                surface: Surface::Page,
                live,
            };
            let height = scene.measure().size(block, width).y;
            scene.paint(block, Rect::from_min_size(pos2(left, y), vec2(width, height)), block_id(base, node, index));
            y += height;
            if !anchored && surface.drawer.anchored() {
                let extra = surface.drawer.extra();
                let mut drawer = std::mem::take(&mut surface.drawer);
                let mut scene = Scene {
                    ui: ctx.ui,
                    canvas: ctx.canvas,
                    env: frame.env,
                    state: surface,
                    textures: frame.textures,
                    actions: &mut actions,
                    keys: frame.keys,
                    slot: tool.slot,
                    surface: Surface::Page,
                    live,
                };
                drawer.paint(&mut scene, y, left, width);
                surface.drawer = drawer;
                y += extra;
            }
            y += spacing;
        }
        surface.end(ctx.ui, &mut actions);
        actions
    }
}

fn blocks(node: &Node) -> Vec<&Node> {
    match node {
        Node::Column { children, .. } => children.iter().collect(),
        other => vec![other],
    }
}

fn spacing(node: &Node) -> f32 {
    match node {
        Node::Column { spacing, .. } => spacing.unwrap_or(GAP),
        _ => 0.0,
    }
}

fn gaps(node: &Node) -> f32 {
    blocks(node).len().saturating_sub(1) as f32 * spacing(node)
}

fn block_id(base: Id, node: &Node, index: usize) -> Id {
    match node {
        Node::Column { .. } => base.with(0).with(index),
        _ => base,
    }
}

fn paint_missing(ctx: &mut PageCtx<'_>, text: &str) {
    let card = Rect::from_min_size(ctx.rect.min, vec2(ctx.rect.width(), MISSING_H));
    glass::paint(ctx.canvas, card, CARD_RADIUS, Glass::TILE);
    ctx.canvas.text(card.center(), Align2::CENTER_CENTER, text, font::callout(), secondary_label());
}
