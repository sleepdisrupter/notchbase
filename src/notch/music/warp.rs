//! Paints the cover through a grid of points that can pull toward its middle, for the beat's squeeze.

use egui::epaint::Vertex;
use egui::{Color32, Mesh, Pos2, Rect, Shape, TextureId, pos2, vec2};

use crate::ui::canvas::Canvas;

const GRID: usize = 18;
const FEATHER: f32 = 1.2;
const PULL: f32 = 0.2;
const SHIMMER: f32 = 0.05;
const SHIMMER_RINGS: f32 = 13.0;

pub(super) struct Warp {
    pub(super) strength: f32,
    pub(super) phase: f32,
}

pub(super) fn paint_warped(
    canvas: &mut Canvas,
    texture: TextureId,
    rect: Rect,
    radius: f32,
    tint: Color32,
    warp: &Warp,
) {
    let mut mesh = Mesh::with_texture(texture);
    let half = rect.size() * 0.5;
    for row in 0..=GRID {
        for column in 0..=GRID {
            let (u, v) = (column as f32 / GRID as f32, row as f32 / GRID as f32);
            let flat = pos2(rect.left() + u * rect.width(), rect.top() + v * rect.height());
            let color = tint.gamma_multiply(coverage(flat, rect, radius));
            mesh.vertices.push(Vertex { pos: warped(flat, rect.center(), half, warp), uv: pos2(u, v), color });
        }
    }
    let stride = (GRID + 1) as u32;
    for row in 0..GRID as u32 {
        for column in 0..GRID as u32 {
            let corner = row * stride + column;
            mesh.add_triangle(corner, corner + 1, corner + stride);
            mesh.add_triangle(corner + 1, corner + stride + 1, corner + stride);
        }
    }
    canvas.add(Shape::mesh(mesh));
}

fn warped(point: Pos2, center: Pos2, half: egui::Vec2, warp: &Warp) -> Pos2 {
    let offset = point - center;
    let reach = vec2(offset.x / half.x.max(1.0), offset.y / half.y.max(1.0)).length() / std::f32::consts::SQRT_2;
    let pull = PULL * warp.strength * (1.0 - reach * reach * 0.5);
    let calm_edges = (1.0 - reach).clamp(0.0, 1.0);
    let shimmer = SHIMMER * warp.strength * calm_edges * (reach * SHIMMER_RINGS - warp.phase).sin();
    center + offset * (1.0 - pull + shimmer)
}

fn coverage(point: Pos2, rect: Rect, radius: f32) -> f32 {
    let radius = radius.min(rect.width().min(rect.height()) * 0.5);
    let inner = rect.shrink(radius);
    let nearest = pos2(point.x.clamp(inner.left(), inner.right()), point.y.clamp(inner.top(), inner.bottom()));
    ((radius - point.distance(nearest)) / FEATHER + 0.5).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calm_warp_leaves_art() {
        let calm = Warp { strength: 0.0, phase: 3.0 };
        let center = pos2(50.0, 50.0);
        let point = pos2(60.0, 45.0);
        assert!(warped(point, center, vec2(15.0, 15.0), &calm).distance(point) < 1e-4);
    }

    #[test]
    fn strong_warp_pulls_inward() {
        let strong = Warp { strength: 1.0, phase: 0.0 };
        let center = pos2(50.0, 50.0);
        let point = pos2(62.0, 50.0);
        let moved = warped(point, center, vec2(15.0, 15.0), &strong);
        assert!(moved.distance(center) < point.distance(center));
    }

    #[test]
    fn rounded_corners_are_cut() {
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(30.0, 30.0));
        assert_eq!(coverage(pos2(0.0, 0.0), rect, 8.0), 0.0);
        assert!(coverage(pos2(15.0, 0.0), rect, 8.0) >= 0.5);
        assert_eq!(coverage(pos2(15.0, 15.0), rect, 8.0), 1.0);
    }
}
