//! Pictures built into the app, like the ones in What's New. The size is read from the PNG header and each
//! picture is decoded once, then kept as a texture.

use std::collections::HashMap;

use egui::{
    Color32, ColorImage, Rect, Shape, TextureFilter, TextureHandle, TextureId, TextureOptions, Vec2, pos2, vec2,
};

use super::canvas::Canvas;
use super::color::white;
use super::glass;
use super::shape::{self, Blob};

pub const LARGEST_SIDE: u32 = 2048;
// Pictures are made at 2x, so a PNG shows at half its pixel size.
const PIXELS_PER_POINT: f32 = 2.0;
const TEXTURE: &str = "notchbase-picture";
const WIDTH_AT: usize = 16;
const HEIGHT_AT: usize = 20;
const EDGE: f32 = 0.1;
const EDGE_WIDTH: f32 = 1.0;
const MISSING: f32 = 0.06;

pub struct Picture {
    png: &'static [u8],
    pixels: [u32; 2],
}

impl Picture {
    // Only release notes with a picture build one, and none have a picture right now.
    #[cfg_attr(not(test), expect(dead_code))]
    pub const fn new(png: &'static [u8]) -> Self {
        Self { png, pixels: [word(png, WIDTH_AT), word(png, HEIGHT_AT)] }
    }

    pub fn size(&self) -> Vec2 {
        vec2(self.pixels[0] as f32, self.pixels[1] as f32) / PIXELS_PER_POINT
    }

    pub fn fitted(&self, width: f32) -> Vec2 {
        let size = self.size();
        size * (width / size.x.max(1.0)).clamp(0.0, 1.0)
    }

    pub fn decode(&self) -> Option<ColorImage> {
        if self.pixels.iter().any(|side| *side > LARGEST_SIDE) {
            return None;
        }
        let image = image::load_from_memory_with_format(self.png, image::ImageFormat::Png).ok()?.into_rgba8();
        let size = [image.width() as usize, image.height() as usize];
        Some(ColorImage::from_rgba_unmultiplied(size, image.as_raw()))
    }

    fn key(&self) -> usize {
        self.png.as_ptr() as usize
    }
}

const fn word(bytes: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

#[derive(Default)]
pub struct Album(HashMap<usize, Option<TextureHandle>>);

impl Album {
    pub fn paint(&mut self, ctx: &egui::Context, canvas: &mut Canvas, picture: &Picture, rect: Rect, radius: f32) {
        let texture = self.0.entry(picture.key()).or_insert_with(|| upload(ctx, picture)).as_ref();
        let rect = snapped(rect, canvas.pixels_per_point());
        match texture.map(TextureHandle::id) {
            Some(texture) => rounded(canvas, texture, rect, radius),
            None => canvas.rect(rect, radius, white(MISSING)),
        }
    }

    pub fn forget(&mut self) {
        self.0.clear();
    }
}

fn upload(ctx: &egui::Context, picture: &Picture) -> Option<TextureHandle> {
    let options = TextureOptions::LINEAR.with_mipmap_mode(Some(TextureFilter::Linear));
    Some(ctx.load_texture(TEXTURE, picture.decode()?, options))
}

fn snapped(rect: Rect, pixels_per_point: f32) -> Rect {
    let snap = |v: f32| (v * pixels_per_point).round() / pixels_per_point;
    Rect::from_min_max(pos2(snap(rect.left()), snap(rect.top())), pos2(snap(rect.right()), snap(rect.bottom())))
}

fn rounded(canvas: &mut Canvas, texture: TextureId, rect: Rect, radius: f32) {
    let pixels_per_point = canvas.pixels_per_point();
    let blob = Blob { rect, top_radius: radius, bottom_radius: radius, ear: 0.0, bleed_top: false, wave: None };
    let outline = shape::outline(&blob, pixels_per_point);
    if outline.len() < 3 || rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    let mut mesh = shape::fill_polygon_with(&outline, rect.center(), |_| Color32::WHITE, pixels_per_point);
    mesh.texture_id = texture;
    for vertex in &mut mesh.vertices {
        let at = (vertex.pos - rect.min) / rect.size();
        vertex.uv = pos2(at.x.clamp(0.0, 1.0), at.y.clamp(0.0, 1.0));
    }
    canvas.add(Shape::mesh(mesh));
    let normals = shape::vertex_normals(&outline);
    canvas.add(Shape::mesh(glass::rim_mesh(&outline, &normals, EDGE_WIDTH, pixels_per_point, |_, _| white(EDGE))));
}

#[cfg(test)]
impl Picture {
    pub fn sample(width: u32, height: u32) -> Self {
        use image::{ImageFormat, Rgba, RgbaImage};

        let image = RgbaImage::from_pixel(width, height, Rgba([20, 40, 60, 255]));
        let mut png = Vec::new();
        image.write_to(&mut std::io::Cursor::new(&mut png), ImageFormat::Png).expect("a png in memory");
        Self::new(png.leak())
    }
}

#[cfg(test)]
mod tests {
    use egui::RawInput;

    use super::*;

    const FRAME: Rect = Rect::from_min_max(pos2(10.0, 20.0), pos2(110.0, 60.0));
    const RADIUS: f32 = 8.0;
    const SCALES: [f32; 4] = [1.0, 1.25, 1.5, 2.0];
    const HEADER: usize = 24;

    fn cut_short(picture: &Picture) -> Picture {
        Picture::new(picture.png[..HEADER + 8].to_vec().leak())
    }

    fn textures(ctx: &egui::Context) -> usize {
        ctx.tex_manager().read().num_allocated()
    }

    fn painted(ctx: &egui::Context, album: &mut Album, pictures: &[&Picture]) -> Vec<Shape> {
        let mut shapes = Vec::new();
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            let mut canvas = Canvas::new(ui.painter());
            for picture in pictures {
                album.paint(ui.ctx(), &mut canvas, picture, FRAME, RADIUS);
            }
            shapes = std::mem::take(&mut canvas.shapes);
        });
        output.textures_delta.clear();
        shapes
    }

    fn textured(shapes: &[Shape]) -> usize {
        shapes
            .iter()
            .filter(|shape| matches!(shape, Shape::Mesh(mesh) if mesh.texture_id != TextureId::default()))
            .count()
    }

    #[test]
    fn size_comes_from_the_header() {
        let picture = Picture::sample(680, 244);
        assert_eq!(picture.size(), vec2(340.0, 122.0));
        assert_eq!(picture.decode().expect("a png").size, [680, 244]);
        assert_eq!(cut_short(&picture).size(), vec2(340.0, 122.0));
        assert!(cut_short(&picture).decode().is_none());
    }

    #[test]
    fn never_larger_than_life() {
        let picture = Picture::sample(680, 244);
        assert_eq!(picture.fitted(1000.0), picture.size());
        assert_eq!(picture.fitted(340.0), picture.size());
        assert_eq!(picture.fitted(170.0), vec2(170.0, 61.0));
        assert_eq!(picture.fitted(-4.0), Vec2::ZERO);
    }

    #[test]
    fn oversized_picture_is_refused() {
        let wide = Picture::sample(LARGEST_SIDE + 1, 1);
        assert!(image::load_from_memory(wide.png).is_ok());
        assert!(wide.decode().is_none());
        assert!(Picture::sample(LARGEST_SIDE, 1).decode().is_some());
    }

    #[test]
    fn album_uploads_each_picture_once() {
        let ctx = egui::Context::default();
        let (mut album, picture) = (Album::default(), Picture::sample(64, 32));
        painted(&ctx, &mut album, &[]);
        let before = textures(&ctx);
        let shapes = painted(&ctx, &mut album, &[&picture, &picture]);
        assert_eq!((textured(&shapes), shapes.len()), (2, 4));
        assert_eq!(textures(&ctx), before + 1);
        painted(&ctx, &mut album, &[&picture]);
        assert_eq!(textures(&ctx), before + 1);
        album.forget();
        assert_eq!(textures(&ctx), before);
    }

    #[test]
    fn broken_picture_shows_a_quiet_block() {
        let ctx = egui::Context::default();
        let (mut album, broken) = (Album::default(), cut_short(&Picture::sample(64, 32)));
        painted(&ctx, &mut album, &[]);
        let before = textures(&ctx);
        for _ in 0..2 {
            let shapes = painted(&ctx, &mut album, &[&broken]);
            assert!(matches!(shapes.as_slice(), [Shape::Rect(block)] if block.fill == white(MISSING)));
        }
        assert_eq!(textures(&ctx), before);
        assert!(album.0.values().all(Option::is_none) && album.0.len() == 1);
    }

    #[test]
    fn picture_fills_its_frame() {
        let ctx = egui::Context::default();
        let (mut album, picture) = (Album::default(), Picture::sample(64, 32));
        let shapes = painted(&ctx, &mut album, &[&picture]);
        let Some(Shape::Mesh(mesh)) = shapes.first() else { panic!("a textured mesh") };
        let bounds = mesh.calc_bounds();
        assert!(bounds.expand(0.01).contains_rect(FRAME) && FRAME.expand(1.0).contains_rect(bounds), "{bounds:?}");
        let (mut least, mut most) = (pos2(1.0, 1.0), pos2(0.0, 0.0));
        for vertex in &mesh.vertices {
            least = least.min(vertex.uv);
            most = most.max(vertex.uv);
        }
        assert_eq!((least, most), (pos2(0.0, 0.0), pos2(1.0, 1.0)));
    }

    #[test]
    fn frame_snaps_to_whole_pixels() {
        for scale in SCALES {
            for left in [0.0, 16.0, 170.3, 212.5] {
                let rect = snapped(Rect::from_min_size(pos2(left, left * 0.7 + 3.1), vec2(340.0, 122.0)), scale);
                for edge in [rect.left(), rect.top(), rect.right(), rect.bottom()] {
                    let pixel = edge * scale;
                    assert!((pixel - pixel.round()).abs() < 1e-3, "scale {scale}: edge at {pixel}");
                }
                assert!((rect.left() - left).abs() <= 0.5 / scale + 1e-3);
            }
        }
    }
}
