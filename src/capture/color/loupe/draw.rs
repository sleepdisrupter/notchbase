//! Paints the loupe: its shadows, the magnified cells, the grid, the center mark and bezel, and then the pill,
//! which fades across to the other side when it flips.

use egui::{Color32, Painter, Pos2, Rect, Shape, TextureId, Vec2, vec2};

use super::lens::{BEZEL_RADIUS, LENS_RADIUS, center_mark, grid_mesh, magnifier, snap_cell, snap_center, soft_shadow};
use super::{Loupe, pill};
use crate::capture::overlay::glide::settle;
use crate::platform::Capture;
use crate::ui::canvas::Canvas;
use crate::ui::font;
use crate::ui::glass::{self, Glass};
use crate::ui::spring::smoothstep;

const GRID_ALPHA: f32 = 0.25;

fn hex(c: Color32) -> String {
    format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b())
}

pub(in crate::capture::color) struct Scene<'a> {
    pub(in crate::capture::color) texture: TextureId,
    pub(in crate::capture::color) grab: &'a Capture,
    pub(in crate::capture::color) center: Pos2,
    pub(in crate::capture::color) bounds: Rect,
    pub(in crate::capture::color) dt: f32,
    pub(in crate::capture::color) opacity: f32,
    pub(in crate::capture::color) check: f32,
    pub(in crate::capture::color) picked: Option<Color32>,
}

impl Scene<'_> {
    fn middle(&self) -> [i64; 2] {
        [(self.grab.width / 2) as i64, (self.grab.height / 2) as i64]
    }
}

struct Focus {
    center: Pos2,
    cell: f32,
    swatch: Color32,
}

impl Loupe {
    pub(in crate::capture::color) fn draw(&mut self, painter: &Painter, scene: &Scene) {
        let ppp = painter.pixels_per_point();
        let px = scene.middle();
        settle(&mut self.scale, scene.dt);
        settle(&mut self.zoom, scene.dt);
        // Snapped cells are crisp but would step while zooming, so only snap once the zoom settles.
        let cell = if self.zoom.is_settled() { snap_cell(self.zoom.value, ppp) } else { self.zoom.value };
        let [r, g, b] = scene.grab.pixel(px[0], px[1]);
        let focus = Focus { center: snap_center(scene.center, cell, ppp), cell, swatch: Color32::from_rgb(r, g, b) };
        let hex = hex(scene.picked.unwrap_or_else(|| scene.grab.srgb(px[0], px[1])));
        self.draw_lens(painter, scene, &focus);
        self.draw_pills(painter, scene, &focus, &hex);
    }

    fn draw_lens(&mut self, painter: &Painter, scene: &Scene, focus: &Focus) {
        let ppp = painter.pixels_per_point();
        let Focus { center, cell, swatch } = *focus;
        let cap = [scene.grab.width, scene.grab.height];
        let mut canvas = Canvas::new(painter);
        soft_shadow(&mut canvas, center + vec2(0.0, 3.0), BEZEL_RADIUS - 3.0, 26.0, 0.26);
        soft_shadow(&mut canvas, center + vec2(0.0, 1.0), BEZEL_RADIUS - 3.0, 7.0, 0.22);
        canvas.add(Shape::mesh(magnifier(scene.texture, center, LENS_RADIUS + 2.0, scene.middle(), cell, cap)));
        let grid = smoothstep(7.0, 8.0, cell) * GRID_ALPHA;
        if grid > 0.0 {
            let gray = Color32::from_rgba_unmultiplied(128, 128, 128, (grid * 255.0).round() as u8);
            canvas.add(Shape::mesh(grid_mesh(center, cell, ppp, LENS_RADIUS + 1.0, gray)));
        }
        canvas.add(Shape::mesh(center_mark(center, cell, ppp)));
        let bezel = Glass::tinted_by(self.ring_tint.update(swatch, scene.dt));
        glass::ring(&mut canvas, center, LENS_RADIUS, BEZEL_RADIUS, bezel);
        canvas.paint(painter, center, self.scale.value, Vec2::ZERO, scene.opacity);
    }

    fn draw_pills(&mut self, painter: &Painter, scene: &Scene, focus: &Focus, hex: &str) {
        let ppp = painter.pixels_per_point();
        let font = font::value();
        let size = pill::size(Canvas::new(painter).text_width(hex, font.clone()));
        let above = if pill::overflows_below(focus.center, size, scene.bounds) { 1.0 } else { 0.0 };
        let flip = self.flip.follow(above, scene.dt).clamp(0.0, 1.0);
        let glass = Glass::tinted_by(self.pill_tint.update(average(scene.grab), scene.dt));
        let label = pill::Label { glass, swatch: focus.swatch, hex, font, check: scene.check };
        for (above, weight, toward_lens) in [(false, 1.0 - flip, -1.0), (true, flip, 1.0)] {
            if weight <= 0.001 {
                continue;
            }
            let mut canvas = Canvas::new(painter);
            pill::paint(&mut canvas, pill::rect(focus.center, size, scene.bounds, above, ppp), &label);
            let offset = vec2(0.0, toward_lens * 8.0 * (1.0 - weight));
            canvas.paint(painter, focus.center, self.scale.value, offset, scene.opacity * weight);
        }
    }
}

/// The mean color of the whole grab, which tints the pill.
fn average(grab: &Capture) -> Color32 {
    let mut sum = [0u64; 3];
    for px in grab.rgba.chunks_exact(4) {
        for (s, v) in sum.iter_mut().zip(px) {
            *s += *v as u64;
        }
    }
    let n = (grab.rgba.len() / 4).max(1) as u64;
    let [r, g, b] = sum.map(|s| ((s + n / 2) / n) as u8);
    Color32::from_rgb(r, g, b)
}

#[cfg(test)]
mod tests {
    use egui::{ColorImage, TextureOptions, pos2};

    use super::super::placement::{CENTER, WINDOW};
    use super::super::zoom::{MAX_CELL, MIN_CELL};
    use super::super::{GRAB, ZOOM};
    use super::*;

    #[test]
    fn pill_tint_is_rounded_grab_mean() {
        let rgba = [[0, 10, 255, 255], [1, 11, 0, 255], [2, 13, 0, 255]].concat();
        let grab = Capture { width: 3, height: 1, rgba };
        assert_eq!(average(&grab), Color32::from_rgb(1, 11, 85));
    }

    #[test]
    fn paints_every_state_headless() {
        let ctx = egui::Context::default();
        crate::ui::font::install(&ctx);
        let rgba: Vec<u8> =
            (0..GRAB * GRAB).flat_map(|i| [(i % GRAB * 8) as u8, (i / GRAB * 8) as u8, 128, 255]).collect();
        let grab = Capture { width: GRAB, height: GRAB, rgba };
        let image = ColorImage::from_rgba_unmultiplied([GRAB, GRAB], &grab.rgba);
        let texture = ctx.load_texture("grab", image, TextureOptions::NEAREST);
        let window = Rect::from_min_size(Pos2::ZERO, WINDOW);
        let cases = [
            (window, 0.0, None, 0.3),
            (Rect::from_min_max(pos2(CENTER.x, 0.0), WINDOW.to_pos2()), 0.0, None, 1.0),
            (Rect::from_min_max(Pos2::ZERO, pos2(WINDOW.x, 150.0)), 0.0, None, 1.0),
            (window, 1.0, Some(Color32::from_rgb(1, 2, 3)), 0.5),
        ];
        let mut loupe = Loupe::new();
        for ppp in [1.0, 1.25, 2.0] {
            loupe.open(ppp);
            for (bounds, check, picked, opacity) in cases {
                for zoom in [MIN_CELL, MAX_CELL] {
                    loupe.zoom.animate_to(zoom, ZOOM);
                    for _ in 0..3 {
                        let mut input = egui::RawInput { screen_rect: Some(window), ..Default::default() };
                        input.viewports.entry(egui::ViewportId::ROOT).or_default().native_pixels_per_point = Some(ppp);
                        let scene = Scene {
                            texture: texture.id(),
                            grab: &grab,
                            center: CENTER,
                            bounds,
                            dt: 1.0 / 60.0,
                            opacity,
                            check,
                            picked,
                        };
                        let mut output = ctx.run_ui(input, |ui| loupe.draw(ui.painter(), &scene));
                        output.textures_delta.clear();
                        let primitives = ctx.tessellate(std::mem::take(&mut output.shapes), output.pixels_per_point);
                        assert!(!primitives.is_empty());
                    }
                }
            }
        }
    }
}
