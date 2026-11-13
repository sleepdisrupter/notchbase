//! File and app icons for the rows. Each fades in when it arrives, with a glyph for its kind until then.

use std::collections::HashMap;
use std::sync::Arc;

use egui::epaint::RectShape;
use egui::{Color32, ColorImage, ImageData, Pos2, Rect, TextureFilter, TextureHandle, TextureOptions, Vec2, pos2};

use crate::bridge::{FindRow, Kind};
use crate::ui::canvas::Canvas;
use crate::ui::glyph::Glyph;
use crate::ui::motion;
use crate::ui::spring::{Spring, UNIT_PRECISION};

const SHEEN: [f32; 4] = [1.0, 0.5, 0.0, 0.2];
const TEXTURE: &str = "notchbase-find-icon";
const SHOWN: f32 = 1.0;

struct Icon {
    source: Arc<ColorImage>,
    texture: TextureHandle,
    shown: Spring,
}

#[derive(Default)]
pub(super) struct Icons(HashMap<u64, Icon>);

impl Icons {
    pub(super) fn step(&mut self, rows: &[FindRow], pins: &[FindRow], dt: f32) {
        self.0.retain(|key, icon| {
            let drawn =
                |row: &FindRow| row.key == *key && row.icon.as_ref().is_some_and(|it| Arc::ptr_eq(it, &icon.source));
            rows.iter().chain(pins).any(drawn)
        });
        self.0.values_mut().for_each(|icon| icon.shown.step(dt));
    }

    pub(super) fn is_animating(&self) -> bool {
        self.0.values().any(|icon| !icon.shown.is_settled())
    }

    pub(super) fn shown(&self, key: u64) -> f32 {
        self.0.get(&key).map_or(0.0, |icon| icon.shown.value.clamp(0.0, 1.0))
    }

    pub(super) fn paint(&mut self, ctx: &egui::Context, canvas: &mut Canvas, row: &FindRow, center: Pos2, size: f32) {
        let Some(source) = &row.icon else { return };
        if self.0.get(&row.key).is_none_or(|icon| !Arc::ptr_eq(&icon.source, source)) {
            let options = TextureOptions::LINEAR.with_mipmap_mode(Some(TextureFilter::Linear));
            let texture = ctx.load_texture(TEXTURE, ImageData::Color(Arc::clone(source)), options);
            let mut shown = Spring::new(0.0, motion::HOVER, UNIT_PRECISION);
            shown.animate_to(SHOWN, motion::HOVER);
            self.0.insert(row.key, Icon { source: Arc::clone(source), texture, shown });
        }
        let icon = &self.0[&row.key];
        let rect = Rect::from_center_size(center, Vec2::splat(size));
        let uv = Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0));
        let tint = Color32::WHITE.gamma_multiply(icon.shown.value.clamp(0.0, 1.0));
        canvas.add(RectShape::filled(rect, 0.0, tint).with_texture(icon.texture.id(), uv));
    }
}

impl Icons {
    pub(super) fn shine(&self, canvas: &mut Canvas, key: u64, center: Pos2, size: f32, light: f32) {
        let Some(icon) = self.0.get(&key).filter(|_| light > 0.0) else { return };
        let rect = Rect::from_center_size(center, Vec2::splat(size));
        let landed = icon.shown.value.clamp(0.0, 1.0);
        let lit = |share: f32| {
            let level = (255.0 * (light * share * landed).clamp(0.0, 1.0)).round() as u8;
            Color32::from_rgba_premultiplied(level, level, level, 0)
        };
        let corners = [rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom()];
        let within = [pos2(0.0, 0.0), pos2(1.0, 0.0), pos2(1.0, 1.0), pos2(0.0, 1.0)];
        let mut sheen = egui::Mesh::with_texture(icon.texture.id());
        for ((pos, uv), share) in corners.into_iter().zip(within).zip(SHEEN) {
            sheen.vertices.push(egui::epaint::Vertex { pos, uv, color: lit(share) });
        }
        sheen.indices = vec![0, 1, 2, 0, 2, 3];
        canvas.add(egui::Shape::mesh(sheen));
    }
}

pub(super) fn fallback(row: &FindRow) -> Glyph {
    if row.app || row.kind == Kind::App {
        return Glyph::AppIcon;
    }
    if row.folder {
        return Glyph::Folder;
    }
    match row.kind {
        Kind::Folder => Glyph::Folder,
        Kind::Image => Glyph::Photo,
        Kind::Video => Glyph::Film,
        Kind::Audio => Glyph::Waveform,
        Kind::App => Glyph::AppIcon,
        Kind::Link => Glyph::Link,
        Kind::Disk => Glyph::Layers,
        Kind::Document
        | Kind::Pdf
        | Kind::Archive
        | Kind::Code
        | Kind::Text
        | Kind::Sheet
        | Kind::Slides
        | Kind::Font
        | Kind::Other => Glyph::Document,
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn row(kind: Kind, folder: bool) -> FindRow {
        FindRow {
            key: 7,
            path: Arc::from(Path::new("/x")),
            name: "x".into(),
            place: "~".into(),
            marks: Vec::new(),
            kind,
            app: false,
            folder,
            cloud: false,
            modified: 0,
            why: None,
            predicted: false,
            icon: None,
        }
    }

    #[test]
    fn missing_icon_gets_kind_glyph() {
        assert_eq!(fallback(&row(Kind::Image, false)), Glyph::Photo);
        assert_eq!(fallback(&row(Kind::Other, true)), Glyph::Folder);
        assert_eq!(fallback(&row(Kind::App, false)), Glyph::AppIcon);
        assert_eq!(fallback(&row(Kind::Pdf, false)), Glyph::Document);
        assert_eq!(fallback(&row(Kind::App, true)), Glyph::AppIcon, "a bundle is an app before it is a folder");
        let launched = |kind, folder| FindRow { app: true, ..row(kind, folder) };
        assert_eq!(fallback(&launched(Kind::App, true)), Glyph::AppIcon);
        assert_eq!(fallback(&launched(Kind::Link, false)), Glyph::AppIcon, "what launches is an app, whatever it is");
    }

    #[test]
    fn icon_fades_in_once() {
        let ctx = egui::Context::default();
        let mut icons = Icons::default();
        let mut with = row(Kind::Pdf, false);
        with.icon = Some(Arc::new(ColorImage::filled([2, 2], egui::Color32::RED)));
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let mut canvas = Canvas::new(ui.painter());
            icons.paint(ui.ctx(), &mut canvas, &with, Pos2::ZERO, 32.0);
        });
        output.textures_delta.clear();
        assert_eq!(icons.shown(7), 0.0);
        assert!(icons.is_animating());
        for _ in 0..120 {
            icons.step(std::slice::from_ref(&with), &[], 1.0 / 60.0);
        }
        assert_eq!(icons.shown(7), 1.0);
        icons.step(&[], &[], 1.0 / 60.0);
        assert_eq!(icons.shown(7), 0.0);
    }
}
