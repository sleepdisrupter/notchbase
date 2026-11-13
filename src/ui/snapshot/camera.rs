use egui::epaint::ClippedPrimitive;
use egui::{ColorImage, CursorIcon, PlatformOutput, Pos2, Rect, TextureId, Vec2};

use super::{Atlas, rasterize};
use crate::ui::font;

const LARGEST_TEXTURE: usize = 16_384;

pub struct Camera {
    ctx: egui::Context,
    size: Vec2,
    scale: f32,
    atlas: Atlas,
    frosted: bool,
    asked: PlatformOutput,
}

impl Camera {
    pub fn new(size: Vec2, scale: f32) -> Self {
        let ctx = egui::Context::default();
        font::install(&ctx);
        Self { ctx, size, scale, atlas: Atlas::default(), frosted: false, asked: PlatformOutput::default() }
    }

    pub fn sits_on_frost(&mut self, frosted: bool) {
        self.frosted = frosted;
    }

    pub fn texture(&self, id: TextureId) -> Option<&ColorImage> {
        self.atlas.0.get(&id)
    }

    pub fn cursor(&self) -> CursorIcon {
        self.asked.cursor_icon
    }

    pub fn asked_nothing_of_the_clipboard(&self) -> bool {
        self.asked.commands.is_empty()
    }

    pub fn pass(&mut self, events: Vec<egui::Event>, ui: impl FnMut(&mut egui::Ui)) -> Vec<ClippedPrimitive> {
        let mut input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
            max_texture_side: Some(LARGEST_TEXTURE),
            events,
            ..Default::default()
        };
        input.viewports.entry(egui::ViewportId::ROOT).or_default().native_pixels_per_point = Some(self.scale);
        let mut output = self.ctx.run_ui(input, ui);
        self.atlas.keep(&mut output);
        self.asked = std::mem::take(&mut output.platform_output);
        self.ctx.tessellate(std::mem::take(&mut output.shapes), output.pixels_per_point)
    }

    pub fn shoot(&mut self, events: Vec<egui::Event>, ui: impl FnMut(&mut egui::Ui)) -> image::RgbImage {
        let primitives = self.pass(events, ui);
        let frame = rasterize(&primitives, &self.atlas, self.size, self.scale);
        if self.frosted { frame.over_frosted_desk() } else { frame.over_desk() }
    }
}

#[cfg(test)]
mod tests {
    use egui::{Color32, vec2};

    use super::*;

    #[test]
    fn camera_paints_at_size_and_scale() {
        let mut camera = Camera::new(vec2(20.0, 10.0), 2.0);
        let shot = camera.shoot(Vec::new(), |ui| {
            ui.painter().rect_filled(Rect::from_min_size(Pos2::ZERO, vec2(10.0, 10.0)), 0.0, Color32::WHITE);
        });
        assert_eq!(shot.dimensions(), (40, 20));
        assert_eq!(shot.get_pixel(5, 10).0, [255, 255, 255]);
        assert_ne!(shot.get_pixel(35, 10).0, [255, 255, 255], "the desk shows where nothing is painted");
    }

    #[test]
    fn camera_remembers_last_requests() {
        let mut camera = Camera::new(vec2(20.0, 10.0), 2.0);
        camera.pass(Vec::new(), |ui| {
            ui.ctx().set_cursor_icon(CursorIcon::Text);
            ui.ctx().copy_text("words".to_owned());
        });
        assert!(camera.cursor() == CursorIcon::Text && !camera.asked_nothing_of_the_clipboard());
        camera.pass(Vec::new(), |_| {});
        assert!(camera.cursor() == CursorIcon::Default && camera.asked_nothing_of_the_clipboard());
    }
}
