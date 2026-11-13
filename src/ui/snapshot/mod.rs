mod camera;

use std::collections::HashMap;
use std::path::PathBuf;

use egui::epaint::{ClippedPrimitive, ImageDelta, Primitive, Vertex};
use egui::{ColorImage, ImageData, Pos2, TextureId, Vec2};

use super::erase;

pub use camera::Camera;

const FOLDER: &str = "NOTCHBASE_SNAPSHOTS";
const DEFAULT_FOLDER: &str = "target/snapshots";
const DESK: &str = "NOTCHBASE_SNAPSHOT_DESK";
const DESK_TOP: [f32; 3] = [86.0, 112.0, 168.0];
const DESK_BOTTOM: [f32; 3] = [196.0, 150.0, 132.0];
const BRIGHT_TOP: [f32; 3] = [203.0, 204.0, 190.0];
const BRIGHT_BOTTOM: [f32; 3] = [160.0, 164.0, 150.0];
const DARK_TOP: [f32; 3] = [19.0, 19.0, 19.0];
const DARK_BOTTOM: [f32; 3] = [14.0, 14.0, 14.0];
const FROST: [f32; 3] = [34.0, 34.0, 36.0];
const FROST_DEPTH: f32 = 0.74;

pub struct Frame {
    size: [usize; 2],
    pixels: Vec<[f32; 4]>,
    smoke: Vec<f32>,
}

impl Frame {
    fn new(size: [usize; 2]) -> Self {
        Self { size, pixels: vec![[0.0; 4]; size[0] * size[1]], smoke: vec![0.0; size[0] * size[1]] }
    }

    fn blend(&mut self, x: usize, y: usize, src: [f32; 4]) {
        let dst = &mut self.pixels[y * self.size[0] + x];
        let keep = 1.0 - src[3] / 255.0;
        for (d, s) in dst.iter_mut().zip(src) {
            *d = s + *d * keep;
        }
    }

    fn erase(&mut self, x: usize, y: usize, strength: f32) {
        for channel in &mut self.pixels[y * self.size[0] + x] {
            *channel *= 1.0 - strength;
        }
    }

    fn pane(&mut self, x: usize, y: usize, strength: f32) {
        let smoke = &mut self.smoke[y * self.size[0] + x];
        *smoke = smoke.max(strength);
    }

    pub fn over_desk(&self) -> image::RgbImage {
        let ends = desk_ends(&std::env::var(DESK).unwrap_or_default());
        self.over(|down, smoke| desk(ends, down).map(|seen| 255.0 * smoked(seen / 255.0, smoke)))
    }

    pub fn over_frosted_desk(&self) -> image::RgbImage {
        let desk = |down| desk((DESK_TOP, DESK_BOTTOM), down);
        self.over(|down, _| std::array::from_fn(|i| desk(down)[i] + (FROST[i] - desk(down)[i]) * FROST_DEPTH))
    }

    fn over(&self, behind: impl Fn(f32, f32) -> [f32; 3]) -> image::RgbImage {
        let [w, h] = self.size;
        image::RgbImage::from_fn(w as u32, h as u32, |x, y| {
            let index = y as usize * w + x as usize;
            let under = behind(y as f32 / h as f32, self.smoke[index]);
            let px = self.pixels[index];
            let keep = 1.0 - px[3] / 255.0;
            image::Rgb(std::array::from_fn(|i| (px[i] + under[i] * keep).round() as u8))
        })
    }
}

fn desk((top, bottom): ([f32; 3], [f32; 3]), down: f32) -> [f32; 3] {
    std::array::from_fn(|i| top[i] + (bottom[i] - top[i]) * down)
}

fn desk_ends(name: &str) -> ([f32; 3], [f32; 3]) {
    match name.trim().to_ascii_lowercase().as_str() {
        "bright" => (BRIGHT_TOP, BRIGHT_BOTTOM),
        "dark" => (DARK_TOP, DARK_BOTTOM),
        _ => (DESK_TOP, DESK_BOTTOM),
    }
}

fn smoked(behind: f32, strength: f32) -> f32 {
    erase::SMOKE.iter().fold(behind, |seen, (gray, opacity)| seen + (seen.min(*gray) - seen) * opacity * strength)
}

#[derive(Default)]
pub struct Atlas(HashMap<TextureId, ColorImage>);

impl Atlas {
    pub fn keep(&mut self, output: &mut egui::FullOutput) {
        for (id, deltas) in &output.textures_delta.set {
            deltas.iter().for_each(|delta| self.apply(*id, delta));
        }
        output.textures_delta.clear();
    }

    fn apply(&mut self, id: TextureId, delta: &ImageDelta) {
        let ImageData::Color(image) = &delta.image;
        match delta.pos {
            None => {
                self.0.insert(id, ColorImage::clone(image));
            }
            Some([left, top]) => {
                let Some(whole) = self.0.get_mut(&id) else { return };
                for row in 0..image.size[1] {
                    let from = row * image.size[0];
                    let to = (top + row) * whole.size[0] + left;
                    whole.pixels[to..to + image.size[0]].copy_from_slice(&image.pixels[from..from + image.size[0]]);
                }
            }
        }
    }

    fn sample(&self, id: TextureId, u: f32, v: f32) -> [f32; 4] {
        let Some(image) = self.0.get(&id) else { return [255.0; 4] };
        let [w, h] = image.size;
        let (x, y) = (u * w as f32 - 0.5, v * h as f32 - 0.5);
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0, y - y0);
        let at = |dx: f32, dy: f32| {
            let cx = (x0 + dx).clamp(0.0, w as f32 - 1.0) as usize;
            let cy = (y0 + dy).clamp(0.0, h as f32 - 1.0) as usize;
            image.pixels[cy * w + cx].to_array().map(f32::from)
        };
        let (a, b, c, d) = (at(0.0, 0.0), at(1.0, 0.0), at(0.0, 1.0), at(1.0, 1.0));
        std::array::from_fn(|i| {
            let top = a[i] + (b[i] - a[i]) * fx;
            let bottom = c[i] + (d[i] - c[i]) * fx;
            top + (bottom - top) * fy
        })
    }
}

struct Lens<'a> {
    atlas: &'a Atlas,
    scale: f32,
}

fn edge(a: [f32; 2], b: [f32; 2], p: [f32; 2]) -> f32 {
    (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
}

fn owns(a: [f32; 2], b: [f32; 2]) -> bool {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    dy > 0.0 || (dy == 0.0 && dx < 0.0)
}

fn triangle(frame: &mut Frame, lens: &Lens, texture: TextureId, clip: [usize; 4], mut v: [Vertex; 3]) {
    let at = |vertex: &Vertex| [vertex.pos.x * lens.scale, vertex.pos.y * lens.scale];
    if edge(at(&v[0]), at(&v[1]), at(&v[2])) < 0.0 {
        v.swap(1, 2);
    }
    let p = [at(&v[0]), at(&v[1]), at(&v[2])];
    let area = edge(p[0], p[1], p[2]);
    if area <= 0.0 {
        return;
    }
    let low = |i: usize| p.iter().map(|q| q[i]).fold(f32::MAX, f32::min).floor().max(0.0) as usize;
    let high = |i: usize| p.iter().map(|q| q[i]).fold(f32::MIN, f32::max).ceil().max(0.0) as usize;
    let (x0, x1) = (low(0).max(clip[0]), high(0).min(clip[2]));
    let (y0, y1) = (low(1).max(clip[1]), high(1).min(clip[3]));
    let colors = v.map(|vertex| vertex.color.to_array().map(f32::from));
    for y in y0..y1 {
        for x in x0..x1 {
            let at = [x as f32 + 0.5, y as f32 + 0.5];
            let w = [edge(p[1], p[2], at), edge(p[2], p[0], at), edge(p[0], p[1], at)];
            let sides = [(p[1], p[2]), (p[2], p[0]), (p[0], p[1])];
            if w.iter().zip(sides).any(|(e, (a, b))| *e < 0.0 || (*e == 0.0 && !owns(a, b))) {
                continue;
            }
            let w = w.map(|e| e / area);
            let mix = |f: &dyn Fn(usize) -> f32| w[0] * f(0) + w[1] * f(1) + w[2] * f(2);
            let texel = lens.atlas.sample(texture, mix(&|i| v[i].uv.x), mix(&|i| v[i].uv.y));
            if erase::erases(texture) {
                let strength = mix(&|i| colors[i][3]) / 255.0;
                frame.erase(x, y, strength);
                if texture == erase::PANE {
                    frame.pane(x, y, strength);
                }
                continue;
            }
            frame.blend(x, y, std::array::from_fn(|c| mix(&|i| colors[i][c]) * texel[c] / 255.0));
        }
    }
}

pub fn rasterize(primitives: &[ClippedPrimitive], atlas: &Atlas, canvas: Vec2, scale: f32) -> Frame {
    let size = [(canvas.x * scale) as usize, (canvas.y * scale) as usize];
    let lens = Lens { atlas, scale };
    let mut frame = Frame::new(size);
    for clipped in primitives {
        let Primitive::Mesh(mesh) = &clipped.primitive else { continue };
        let bound = |value: f32, most: usize| ((value * scale).round().max(0.0) as usize).min(most);
        let clip = [
            bound(clipped.clip_rect.left(), size[0]),
            bound(clipped.clip_rect.top(), size[1]),
            bound(clipped.clip_rect.right(), size[0]),
            bound(clipped.clip_rect.bottom(), size[1]),
        ];
        for corners in mesh.indices.as_chunks::<3>().0 {
            let vertices = [corners[0], corners[1], corners[2]].map(|i| mesh.vertices[i as usize]);
            triangle(&mut frame, &lens, mesh.texture_id, clip, vertices);
        }
    }
    frame
}

pub fn folder() -> PathBuf {
    let dir = std::env::var_os(FOLDER).map_or_else(|| DEFAULT_FOLDER.into(), PathBuf::from);
    std::fs::create_dir_all(&dir).expect("a folder for snapshots");
    dir
}

#[cfg(test)]
mod tests {
    use egui::{Color32, pos2};

    use super::*;

    #[test]
    fn shared_edge_is_painted_once() {
        let white = |x: f32, y: f32| Vertex { pos: pos2(x, y), uv: Pos2::ZERO, color: Color32::from_white_alpha(128) };
        let corners = [white(0.0, 0.0), white(4.0, 0.0), white(4.0, 4.0), white(0.0, 4.0)];
        let mut frame = Frame::new([8, 8]);
        let atlas = Atlas::default();
        let lens = Lens { atlas: &atlas, scale: 2.0 };
        for [a, b, c] in [[0, 1, 2], [0, 2, 3]] {
            triangle(&mut frame, &lens, TextureId::default(), [0, 0, 8, 8], [corners[a], corners[b], corners[c]]);
        }
        assert!(frame.pixels.iter().all(|px| (px[3] - 128.0).abs() < 0.5), "every pixel is covered exactly once");
    }

    #[test]
    fn smoke_holds_down_the_bright() {
        assert_eq!((smoked(0.0, 1.0), smoked(0.2, 1.0)), (0.0, 0.2));
        assert!((smoked(0.3, 1.0) - 0.25).abs() < 1e-6, "{}", smoked(0.3, 1.0));
        let (sage, white) = (smoked(0.74, 1.0), smoked(1.0, 1.0));
        assert!(sage > 0.25 && sage < white && white < 0.3, "{sage} {white}");
        assert_eq!(smoked(1.0, 0.0), 1.0, "a pane that shows nothing dims nothing");
        assert!(smoked(1.0, 0.5) > white && smoked(1.0, 0.5) < 1.0);
    }
}
