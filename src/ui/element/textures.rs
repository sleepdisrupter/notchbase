//! Textures for tool images. Up to 32 are kept, then the cache starts over.

use std::collections::HashMap;
use std::sync::Arc;

use egui::{ColorImage, TextureHandle, TextureId, TextureOptions};
use notchbase_sdk::protocol::ImageSource;

use crate::tools::ImageStore;

const KEEP: usize = 32;

#[derive(Default)]
pub struct Textures {
    loaded: HashMap<ImageSource, (Arc<ColorImage>, TextureHandle)>,
}

impl Textures {
    pub fn get(&mut self, ctx: &egui::Context, images: &ImageStore, source: &ImageSource) -> Option<TextureId> {
        let image = images.thumbnail(source)?;
        if let Some((cached, handle)) = self.loaded.get(source)
            && Arc::ptr_eq(cached, &image)
        {
            return Some(handle.id());
        }
        if self.loaded.len() >= KEEP {
            self.loaded.clear();
        }
        let handle = ctx.load_texture("tool-image", (*image).clone(), TextureOptions::LINEAR);
        let id = handle.id();
        self.loaded.insert(source.clone(), (image, handle));
        Some(id)
    }
}
