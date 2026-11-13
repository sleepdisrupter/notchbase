//! The lens: snapping cells to physical pixels, the textured fan that magnifies the grab, the grid lines, the mark
//! on the center pixel, and a soft round shadow.

use std::f32::consts::TAU;

use egui::epaint::Vertex;
use egui::{Color32, Mesh, Pos2, Rect, Shape, TextureId, Vec2, pos2, vec2};

use crate::capture::overlay::paint::frame;
use crate::ui::canvas::Canvas;
use crate::ui::color;

pub(super) const LENS_RADIUS: f32 = 60.0;
pub(super) const BEZEL_RADIUS: f32 = 66.0;
const FAN_SEGMENTS: usize = 72;
const SHADOW_SEGMENTS: u32 = 96;
const SHADOW_RINGS: u32 = 8;

pub(super) fn snap_cell(cell: f32, ppp: f32) -> f32 {
    (cell * ppp).round().max(1.0) / ppp
}

/// Moves the center by less than a pixel so that cell edges fall on physical pixels.
pub(super) fn snap_center(p: Pos2, cell: f32, ppp: f32) -> Pos2 {
    let half = cell * ppp * 0.5;
    let snap = |v: f32| ((v * ppp - half).round() + half) / ppp;
    pos2(snap(p.x), snap(p.y))
}

pub(super) fn magnifier(
    texture: TextureId,
    center: Pos2,
    radius: f32,
    px: [i64; 2],
    cell: f32,
    cap: [usize; 2],
) -> Mesh {
    let mut mesh = Mesh::with_texture(texture);
    mesh.reserve_vertices(FAN_SEGMENTS + 1);
    mesh.reserve_triangles(FAN_SEGMENTS);
    let mut vertex = |d: Vec2| {
        mesh.vertices.push(Vertex { pos: center + d, uv: texel_uv(px, d, cell, cap), color: Color32::WHITE });
    };
    vertex(Vec2::ZERO);
    for k in 0..FAN_SEGMENTS {
        let a = TAU * k as f32 / FAN_SEGMENTS as f32;
        vertex(vec2(a.cos(), a.sin()) * radius);
    }
    let n = FAN_SEGMENTS as u32;
    for k in 0..n {
        mesh.add_triangle(0, 1 + k, 1 + (k + 1) % n);
    }
    mesh
}

/// Grid lines clipped to the lens. Vertical lines are cut where they cross the horizontal ones, so the
/// translucent lines never overlap and darken.
pub(super) fn grid_mesh(center: Pos2, cell: f32, ppp: f32, radius: f32, color: Color32) -> Mesh {
    let mut mesh = Mesh::default();
    let line = 1.0 / ppp;
    let n = (radius / cell).ceil() as i32 + 1;
    let offsets: Vec<f32> = (-n..=n).map(|k| (k as f32 + 0.5) * cell).filter(|o| o.abs() < radius).collect();
    for &oy in &offsets {
        let half = half_chord(radius, oy);
        let y = center.y + oy;
        mesh.add_colored_rect(Rect::from_min_max(pos2(center.x - half, y), pos2(center.x + half, y + line)), color);
    }
    for &ox in &offsets {
        let half = half_chord(radius, ox);
        let (x, bottom) = (center.x + ox, center.y + half);
        let mut y = center.y - half;
        for &oy in &offsets {
            let (a, b) = (center.y + oy, center.y + oy + line);
            if b <= y || a >= bottom {
                continue;
            }
            if a > y {
                mesh.add_colored_rect(Rect::from_min_max(pos2(x, y), pos2(x + line, a)), color);
            }
            y = b;
        }
        if bottom > y {
            mesh.add_colored_rect(Rect::from_min_max(pos2(x, y), pos2(x + line, bottom)), color);
        }
    }
    mesh
}

pub(super) fn center_mark(center: Pos2, cell: f32, ppp: f32) -> Mesh {
    let width = ppp.round().max(1.0) / ppp;
    let cell_rect = Rect::from_center_size(center, Vec2::splat(cell));
    let mut mesh = Mesh::default();
    frame(&mut mesh, cell_rect.expand(width), width, Color32::from_black_alpha(150));
    frame(&mut mesh, cell_rect, width, color::white(0.95));
    mesh
}

pub(super) fn soft_shadow(canvas: &mut Canvas, center: Pos2, radius: f32, blur: f32, alpha: f32) {
    let mut mesh = Mesh::default();
    for ring in 0..=SHADOW_RINGS {
        let u = ring as f32 / SHADOW_RINGS as f32;
        let a = alpha * (-4.0 * u * u).exp() * (1.0 - u);
        let color = Color32::from_black_alpha((a * 255.0).round() as u8);
        let r = radius + blur * u;
        for k in 0..SHADOW_SEGMENTS {
            let t = TAU * k as f32 / SHADOW_SEGMENTS as f32;
            mesh.colored_vertex(center + vec2(t.cos(), t.sin()) * r, color);
        }
    }
    for ring in 0..SHADOW_RINGS {
        for k in 0..SHADOW_SEGMENTS {
            let a = ring * SHADOW_SEGMENTS + k;
            let b = ring * SHADOW_SEGMENTS + (k + 1) % SHADOW_SEGMENTS;
            mesh.add_triangle(a, b, a + SHADOW_SEGMENTS);
            mesh.add_triangle(b, b + SHADOW_SEGMENTS, a + SHADOW_SEGMENTS);
        }
    }
    canvas.add(Shape::mesh(mesh));
}

fn texel_uv(px: [i64; 2], d: Vec2, cell: f32, cap: [usize; 2]) -> Pos2 {
    pos2((px[0] as f32 + 0.5 + d.x / cell) / cap[0] as f32, (px[1] as f32 + 0.5 + d.y / cell) / cap[1] as f32)
}

fn half_chord(radius: f32, offset: f32) -> f32 {
    (radius * radius - offset * offset).max(0.0).sqrt()
}

#[cfg(test)]
mod tests {
    use super::super::GRAB;
    use super::super::zoom::{DEFAULT_CELL, MAX_CELL, MIN_CELL};
    use super::*;

    const SCALES: [f32; 8] = [1.0, 1.25, 1.5, 1.75, 2.0, 2.25, 2.5, 3.0];

    fn rects(mesh: &Mesh) -> Vec<Rect> {
        mesh.vertices.chunks_exact(4).map(|v| Rect::from_points(&v.iter().map(|v| v.pos).collect::<Vec<_>>())).collect()
    }

    #[test]
    fn lens_center_shows_the_cursor_pixel() {
        let cap = [GRAB, GRAB];
        let px = [(GRAB / 2) as i64; 2];
        let texel = |d: Vec2, cell: f32| {
            let uv = texel_uv(px, d, cell, cap);
            vec2(uv.x * cap[0] as f32, uv.y * cap[1] as f32)
        };
        for cell in [MIN_CELL, DEFAULT_CELL, 13.0, MAX_CELL] {
            let t = texel(Vec2::ZERO, cell);
            assert!((t.x - 15.5).abs() < 1e-4 && (t.y - 15.5).abs() < 1e-4);
            let t = texel(vec2(cell, -cell), cell);
            assert!((t.x - 16.5).abs() < 1e-4 && (t.y - 14.5).abs() < 1e-4);
            for d in [vec2(-0.49, -0.49), vec2(0.49, 0.49), vec2(0.49, -0.3)] {
                let t = texel(d * cell, cell);
                assert_eq!([t.x.floor() as i64, t.y.floor() as i64], px);
            }
        }
    }

    #[test]
    fn grab_covers_lens_at_smallest_cells() {
        let reach = (LENS_RADIUS + 2.0) / MIN_CELL;
        assert!(reach.ceil() as usize <= GRAB / 2, "{reach} cells from the center");
    }

    #[test]
    fn snapping_puts_cell_edges_on_pixels() {
        for ppp in SCALES {
            for base in [MIN_CELL, DEFAULT_CELL, 13.0, MAX_CELL] {
                let cell = snap_cell(base, ppp);
                for v in [0.0, 17.3, 120.5, 1234.5] {
                    let p = pos2(v, v * 0.7 + 3.1);
                    let c = snap_center(p, cell, ppp);
                    for edge in [(c.x - cell * 0.5) * ppp, (c.y - cell * 0.5) * ppp] {
                        assert!((edge - edge.round()).abs() < 1e-2, "ppp {ppp} cell {cell}: edge {edge}");
                    }
                    assert!((c.x - p.x).abs() <= 0.5 / ppp + 1e-3);
                    assert!((c.y - p.y).abs() <= 0.5 / ppp + 1e-3);
                }
            }
        }
    }

    #[test]
    fn grid_stays_in_lens_without_overlap() {
        let center = pos2(200.0, 150.0);
        for (cell, ppp) in [(8.0, 1.0), (10.0, 2.0), (13.0, 1.5), (20.0, 1.25)] {
            let radius = LENS_RADIUS + 1.0;
            let rects = rects(&grid_mesh(center, cell, ppp, radius, Color32::WHITE));
            assert!(!rects.is_empty());
            for r in &rects {
                for corner in [r.left_top(), r.right_bottom()] {
                    assert!(corner.distance(center) <= radius + 2.0 / ppp);
                }
            }
            for (i, a) in rects.iter().enumerate() {
                for b in &rects[i + 1..] {
                    let overlap = a.intersect(*b);
                    assert!(overlap.width() <= 1e-3 || overlap.height() <= 1e-3, "{a:?} overlaps {b:?}");
                }
            }
        }
    }

    #[test]
    fn center_mark_frames_the_center_cell() {
        let center = snap_center(pos2(100.3, 80.7), 10.0, 2.0);
        let bounds = rects(&center_mark(center, 10.0, 2.0)).into_iter().fold(Rect::NOTHING, Rect::union);
        assert!((bounds.width() - 12.0).abs() < 1e-3);
        assert!((bounds.center() - center).length() < 1e-3);
    }

    #[test]
    fn magnifier_is_a_closed_fan() {
        let mesh = magnifier(TextureId::default(), pos2(50.0, 40.0), LENS_RADIUS, [15, 15], DEFAULT_CELL, [GRAB, GRAB]);
        assert_eq!(mesh.vertices.len(), FAN_SEGMENTS + 1);
        assert_eq!(mesh.indices.len(), FAN_SEGMENTS * 3);
        assert_eq!(mesh.vertices[0].pos, pos2(50.0, 40.0));
        assert!(mesh.vertices[1..].iter().all(|v| (v.pos.distance(pos2(50.0, 40.0)) - LENS_RADIUS).abs() < 1e-3));
    }
}
