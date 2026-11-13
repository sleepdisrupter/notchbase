//! The pages the open panel shows. Each one works out its height for the panel and draws itself into a `PageCtx`.

pub mod agents;
pub mod find;
#[cfg(test)]
pub(crate) mod harness;
pub mod media;
pub mod mixer;
pub mod notifications;
pub mod player;
pub mod settings;
pub mod shelf;
pub mod stats;
pub mod tool;
pub mod tools;
pub mod welcome;
pub mod whats_new;

use std::time::Instant;

use egui::Rect;

use crate::ui::canvas::Canvas;

/// What a page draws with this frame: the ui and canvas, its rect, how far it has faded in, and the time.
pub struct PageCtx<'a> {
    pub ui: &'a mut egui::Ui,
    pub canvas: &'a mut Canvas,
    pub rect: Rect,
    pub alpha: f32,
    pub dt: f32,
    pub now: Instant,
}
