use egui::epaint::{CircleShape, RectShape};
use egui::{Context, Id, RawInput, Rect, Shape};
use notchbase_sdk::protocol::{Node, Surface};

use super::{Env, Prompt, Scene, SurfaceState, Textures};
use crate::bridge::{Action, ToolSlot};
use crate::platform::Style;
use crate::tools::ImageStore;
use crate::ui::canvas::Canvas;
use crate::ui::font;

pub(super) const SLOT: ToolSlot = ToolSlot(4);
pub(super) const SURFACE: Surface = Surface::Page;

pub(super) struct Stage {
    ctx: Context,
    pub(super) images: ImageStore,
    textures: Textures,
    pub(super) state: SurfaceState,
    pub(super) live: bool,
    pub(super) now_ms: u64,
}

pub(super) struct Painted {
    pub(super) shapes: Vec<Shape>,
    pub(super) actions: Vec<Action>,
}

impl Stage {
    pub(super) fn new() -> Self {
        let ctx = Context::default();
        font::install(&ctx);
        Self {
            ctx,
            images: ImageStore::default(),
            textures: Textures::default(),
            state: SurfaceState::default(),
            live: false,
            now_ms: 0,
        }
    }

    pub(super) fn live() -> Self {
        Self { live: true, ..Self::new() }
    }

    pub(super) fn paint(&mut self, node: &Node, rect: Rect) -> Painted {
        self.with(|scene| scene.paint(node, rect, Id::new("notchbase.stage")))
    }

    pub(super) fn with(&mut self, draw: impl FnOnce(&mut Scene)) -> Painted {
        let env = Env {
            style: Style::Island,
            images: &self.images,
            shortcuts: &[],
            prompt: Prompt::default(),
            now_ms: self.now_ms,
        };
        let mut draw = Some(draw);
        let mut painted = Painted { shapes: Vec::new(), actions: Vec::new() };
        let mut output = self.ctx.run_ui(RawInput::default(), |ui| {
            let Some(draw) = draw.take() else { return };
            let mut canvas = Canvas::new(ui.painter());
            let mut keys = Vec::new();
            let mut scene = Scene {
                ui,
                canvas: &mut canvas,
                env: &env,
                state: &mut self.state,
                textures: &mut self.textures,
                actions: &mut painted.actions,
                keys: &mut keys,
                slot: SLOT,
                surface: SURFACE,
                live: self.live,
            };
            draw(&mut scene);
            painted.shapes = std::mem::take(&mut canvas.shapes);
        });
        output.textures_delta.clear();
        painted
    }
}

impl Painted {
    pub(super) fn circles(&self) -> Vec<&CircleShape> {
        self.shapes
            .iter()
            .filter_map(|shape| if let Shape::Circle(circle) = shape { Some(circle) } else { None })
            .collect()
    }

    pub(super) fn rects(&self) -> Vec<&RectShape> {
        self.shapes.iter().filter_map(|shape| if let Shape::Rect(rect) = shape { Some(rect) } else { None }).collect()
    }

    pub(super) fn texts(&self) -> Vec<(Rect, String)> {
        let text = |shape: &Shape| match shape {
            Shape::Text(text) => {
                Some((Rect::from_min_size(text.pos, text.galley.size()), text.galley.text().to_owned()))
            }
            _ => None,
        };
        self.shapes.iter().filter_map(text).collect()
    }

    pub(super) fn lines(&self) -> Vec<&egui::epaint::PathShape> {
        self.shapes.iter().filter_map(|shape| if let Shape::Path(path) = shape { Some(path) } else { None }).collect()
    }
}
