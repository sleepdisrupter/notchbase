//! App icons in color and in gray, with a monogram for an app that has none.

use std::collections::HashMap;
use std::sync::Arc;

use egui::epaint::RectShape;
use egui::{
    Align2, Color32, ColorImage, ImageData, Pos2, Rect, Shape, Stroke, TextureFilter, TextureHandle, TextureOptions,
    pos2, vec2,
};

use crate::audio::{AppId, AudioApp};
use crate::ui::canvas::Canvas;
use crate::ui::color::{self, white};
use crate::ui::font::semibold;

const RADIUS: f32 = 5.0;
const SLASH_INSET: f32 = 2.5;

struct Textures {
    source: Arc<ColorImage>,
    color: TextureHandle,
    gray: TextureHandle,
}

#[derive(Default)]
pub(super) struct Icons(HashMap<AppId, Textures>);

impl Icons {
    pub(super) fn retain(&mut self, keep: impl Fn(AppId) -> bool) {
        self.0.retain(|&id, _| keep(id));
    }

    pub(super) fn paint(
        &mut self,
        ctx: &egui::Context,
        c: &mut Canvas,
        rect: Rect,
        app: &AudioApp,
        mute: f32,
        hover: f32,
    ) {
        match self.textures(ctx, app) {
            Some(textures) => paint_textures(c, rect, textures, mute, hover),
            None => paint_monogram(c, rect, &app.name, mute, hover),
        }
        if mute > 0.01 {
            paint_slash(c, rect, mute);
        }
    }

    fn textures(&mut self, ctx: &egui::Context, app: &AudioApp) -> Option<&Textures> {
        let icon = app.icon.as_ref()?;
        let stale = self.0.get(&app.id).is_none_or(|t| !Arc::ptr_eq(&t.source, icon));
        if stale {
            let options = TextureOptions::LINEAR.with_mipmap_mode(Some(TextureFilter::Linear));
            let color = ctx.load_texture(format!("mixer-icon-{}", app.id), ImageData::Color(Arc::clone(icon)), options);
            let gray = ctx.load_texture(format!("mixer-icon-gray-{}", app.id), grayscale(icon), options);
            self.0.insert(app.id, Textures { source: Arc::clone(icon), color, gray });
        }
        self.0.get(&app.id)
    }
}

fn paint_textures(c: &mut Canvas, rect: Rect, textures: &Textures, mute: f32, hover: f32) {
    let uv = Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0));
    let mut draw = |texture: &TextureHandle, alpha: f32| {
        if alpha > 0.001 {
            c.add(RectShape::filled(rect, RADIUS, white(alpha)).with_texture(texture.id(), uv));
        }
    };
    draw(&textures.color, 1.0 - mute);
    draw(&textures.gray, 0.4 * mute);
    if hover > 0.001 {
        c.rect(rect, RADIUS, white(0.10 * hover));
    }
}

fn paint_monogram(c: &mut Canvas, rect: Rect, name: &str, mute: f32, hover: f32) {
    let center = rect.center();
    let fill = color::mix(white(0.14), white(0.2), hover);
    c.circle(center, rect.width() * 0.5, fill.gamma_multiply(1.0 - 0.5 * mute));
    c.text(center, Align2::CENTER_CENTER, monogram(name), semibold(10.5), white(0.85 * (1.0 - 0.55 * mute)));
}

fn paint_slash(c: &mut Canvas, rect: Rect, mute: f32) {
    let a = rect.left_top() + vec2(SLASH_INSET, SLASH_INSET);
    let b = rect.right_bottom() - vec2(SLASH_INSET, SLASH_INSET);
    let end = a + (b - a) * mute;
    c.add(Shape::line_segment([a, end], Stroke::new(3.0, Color32::from_black_alpha((150.0 * mute) as u8))));
    c.add(Shape::line_segment([a, end], Stroke::new(1.5, white(0.92 * mute))));
}

fn monogram(name: &str) -> String {
    name.chars().find(|c| c.is_alphanumeric()).map_or_else(|| "?".to_owned(), |c| c.to_uppercase().collect())
}

fn grayscale(image: &ColorImage) -> ColorImage {
    let pixels = image
        .pixels
        .iter()
        .map(|p| {
            let y = (0.2126 * p.r() as f32 + 0.7152 * p.g() as f32 + 0.0722 * p.b() as f32).round() as u8;
            Color32::from_rgba_premultiplied(y, y, y, p.a())
        })
        .collect();
    ColorImage::new(image.size, pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monograms() {
        assert_eq!(monogram("spotify"), "S");
        assert_eq!(monogram("  (beta) app"), "B");
        assert_eq!(monogram(""), "?");
    }

    #[test]
    fn grayscale_keeps_alpha() {
        let image =
            ColorImage::new([2, 1], vec![Color32::from_rgba_premultiplied(100, 0, 0, 100), Color32::TRANSPARENT]);
        let gray = grayscale(&image);
        let p = gray.pixels[0];
        assert_eq!((p.r(), p.g(), p.b(), p.a()), (21, 21, 21, 100));
        assert_eq!(gray.pixels[1], Color32::TRANSPARENT);
    }
}
